package util

import (
	"context"
	"fmt"
	"strings"

	"github.com/docker/docker/api/types"
	"github.com/docker/docker/api/types/container"
	"github.com/docker/docker/api/types/filters"
	"github.com/docker/docker/api/types/network"
	"github.com/docker/docker/api/types/volume"
	"github.com/docker/docker/client"
)

// NewDockerClient creates a new Docker client and checks if Docker is available
func NewDockerClient() (*client.Client, error) {
	apiClient, err := client.NewClientWithOpts(client.FromEnv)
	if err != nil {
		return nil, fmt.Errorf("failed to create docker client: %w", err)
	}

	// Test the connection to verify Docker is available
	_, err = apiClient.Ping(context.Background())
	if err != nil {
		return nil, fmt.Errorf("failed to connect to Docker: %w", err)
	}

	return apiClient, nil
}

// createKurtosisFilter creates a filter for kurtosis resources
func createKurtosisFilter(enclave ...string) filters.Args {
	kurtosisFilter := filters.NewArgs()
	if len(enclave) > 0 {
		kurtosisFilter.Add("label", fmt.Sprintf("kurtosis.devnet.enclave=%s", enclave[0]))
	} else {
		kurtosisFilter.Add("label", "kurtosis.devnet.enclave")
	}
	return kurtosisFilter
}

// destroyContainers stops and removes containers matching the filter
func destroyContainers(ctx context.Context, apiClient *client.Client, filter filters.Args) error {
	containers, err := apiClient.ContainerList(ctx, container.ListOptions{
		All:     true,
		Filters: filter,
	})
	if err != nil {
		return fmt.Errorf("failed to list containers: %w", err)
	}

	for _, cont := range containers {
		if cont.State == "running" {
			timeoutSecs := int(10)
			if err := apiClient.ContainerStop(ctx, cont.ID, container.StopOptions{
				Timeout: &timeoutSecs,
			}); err != nil {
				return fmt.Errorf("failed to stop container %s: %w", cont.ID, err)
			}
		}

		if err := apiClient.ContainerRemove(ctx, cont.ID, container.RemoveOptions{
			RemoveVolumes: true,
			Force:         true,
		}); err != nil {
			return fmt.Errorf("failed to remove container %s: %w", cont.ID, err)
		}
	}
	return nil
}

// destroyVolumes removes volumes matching the filter
func destroyVolumes(ctx context.Context, apiClient *client.Client, filter filters.Args) error {
	volumes, err := apiClient.VolumeList(ctx, volume.ListOptions{
		Filters: filter,
	})
	if err != nil {
		return fmt.Errorf("failed to list volumes: %w", err)
	}

	for _, volume := range volumes.Volumes {
		if err := apiClient.VolumeRemove(ctx, volume.Name, true); err != nil {
			return fmt.Errorf("failed to remove volume %s: %w", volume.Name, err)
		}
	}
	return nil
}

// destroyNetworks removes networks matching the filter
func destroyNetworks(ctx context.Context, apiClient *client.Client, enclaveName string) error {
	networks, err := apiClient.NetworkList(ctx, network.ListOptions{})
	if err != nil {
		return fmt.Errorf("failed to list networks: %w", err)
	}

	for _, network := range networks {
		if (enclaveName != "" && strings.HasPrefix(network.Name, fmt.Sprintf("kt-%s-devnet", enclaveName))) ||
			(enclaveName == "" && strings.Contains(network.Name, "kt-")) {
			if err := apiClient.NetworkRemove(ctx, network.ID); err != nil {
				return fmt.Errorf("failed to remove network: %w", err)
			}
		}
	}
	return nil
}

// DestroyDockerResources removes all Docker resources associated with the given enclave
func DestroyDockerResources(ctx context.Context, enclave ...string) error {
	apiClient, err := NewDockerClient()
	if err != nil {
		return err
	}

	enclaveName := ""
	if len(enclave) > 0 {
		enclaveName = enclave[0]
	}
	fmt.Printf("Destroying docker resources for enclave: %s\n", enclaveName)

	filter := createKurtosisFilter(enclave...)

	if err := destroyContainers(ctx, apiClient, filter); err != nil {
		fmt.Printf("failed to destroy containers: %v", err)
	}

	if err := destroyVolumes(ctx, apiClient, filter); err != nil {
		fmt.Printf("failed to destroy volumes: %v", err)
	}

	if err := destroyNetworks(ctx, apiClient, enclaveName); err != nil {
		fmt.Printf("failed to destroy networks: %v", err)
	}

	return nil
}

// FixTraefikNetwork adds a new Docker provider to Traefik with the correct network ID
func FixTraefikNetwork(ctx context.Context) error {
	apiClient, err := NewDockerClient()
	if err != nil {
		return fmt.Errorf("failed to create Docker client: %w", err)
	}

	// Find Traefik container first
	traefikFilters := filters.NewArgs()
	traefikFilters.Add("name", "kurtosis-reverse-proxy")

	traefikContainers, err := apiClient.ContainerList(ctx, container.ListOptions{
		All:     false,
		Filters: traefikFilters,
	})
	if err != nil {
		return fmt.Errorf("failed to list containers: %w", err)
	}

	var traefikContainer *types.Container
	for _, c := range traefikContainers {
		for _, name := range c.Names {
			if strings.Contains(name, "kurtosis-reverse-proxy") {
				traefikContainer = &c
				break
			}
		}
		if traefikContainer != nil {
			break
		}
	}

	if traefikContainer == nil {
		return fmt.Errorf("traefik container (kurtosis-reverse-proxy) not found")
	}

	// Get all network IDs that Traefik is connected to
	networkIDs := make(map[string]bool) // Use map to avoid duplicates
	for networkName, network := range traefikContainer.NetworkSettings.Networks {
		if networkName != "bridge" {
			networkIDs[network.NetworkID] = true
		}
	}

	if len(networkIDs) == 0 {
		return fmt.Errorf("traefik container is not connected to any networks (except bridge)")
	}

	// Convert map keys to slice for easier handling
	var allNetworkIDs []string
	for networkID := range networkIDs {
		allNetworkIDs = append(allNetworkIDs, networkID)
	}

	// Check if user service containers have networks that Traefik doesn't have access to
	if err := checkUserServiceNetworks(ctx, apiClient, networkIDs); err != nil {
		fmt.Printf("Warning: %v\n", err)
	}

	// We already found the Traefik container above

	fmt.Printf("Configuring Traefik to use all available networks: %v\n", allNetworkIDs)

	// Add provider configuration for each network
	var dynamicConfig strings.Builder
	dynamicConfig.WriteString("# Dynamic Traefik configuration for correct networks\n")
	dynamicConfig.WriteString("providers:\n")

	for i, networkID := range allNetworkIDs {
		dynamicConfig.WriteString(fmt.Sprintf(`  dockerDynamic%d:
    endpoint: "unix:///var/run/docker.sock"
    exposedByDefault: false
    network: "%s"
    watch: true
`, i, networkID))
	}

	// Create the dynamic config file in the container
	createCmd := []string{"sh", "-c", "mkdir -p /etc/traefik/dynamic && echo '" + dynamicConfig.String() + "' > /etc/traefik/dynamic/network-fix.yml"}

	execConfig := container.ExecOptions{
		Cmd:          createCmd,
		AttachStdout: true,
		AttachStderr: true,
	}

	execID, err := apiClient.ContainerExecCreate(ctx, traefikContainer.ID, execConfig)
	if err != nil {
		return fmt.Errorf("failed to create exec: %w", err)
	}

	err = apiClient.ContainerExecStart(ctx, execID.ID, container.ExecStartOptions{})
	if err != nil {
		return fmt.Errorf("failed to exec: %w", err)
	}

	fmt.Printf("✓ Added dynamic Docker providers for networks: %v\n", allNetworkIDs)
	return nil
}

// checkUserServiceNetworks checks if user service containers have networks that Traefik doesn't have access to
func checkUserServiceNetworks(ctx context.Context, apiClient *client.Client, traefikNetworkIDs map[string]bool) error {
	// Find user service containers
	userFilters := filters.NewArgs()
	userFilters.Add("label", "com.kurtosistech.container-type=user-service")

	containers, err := apiClient.ContainerList(ctx, container.ListOptions{
		All:     false,
		Filters: userFilters,
	})
	if err != nil {
		return fmt.Errorf("failed to list user service containers: %w", err)
	}

	// Collect all networks that user services are connected to
	userServiceNetworks := make(map[string]bool)
	containerNetworkMap := make(map[string][]string) // container name -> networks

	for _, container := range containers {
		var containerNetworks []string
		for networkName, network := range container.NetworkSettings.Networks {
			if networkName != "bridge" {
				userServiceNetworks[network.NetworkID] = true
				containerNetworks = append(containerNetworks, networkName)
			}
		}
		if len(containerNetworks) > 0 {
			containerName := strings.TrimPrefix(container.Names[0], "/")
			containerNetworkMap[containerName] = containerNetworks
		}
	}

	// Find networks that user services are on but Traefik is not
	unreachableNetworks := make(map[string]bool)
	for networkID := range userServiceNetworks {
		if !traefikNetworkIDs[networkID] {
			unreachableNetworks[networkID] = true
		}
	}

	if len(unreachableNetworks) > 0 {
		fmt.Printf("⚠️  Found user service containers on networks that Traefik cannot reach:\n")

		// Show which containers are on unreachable networks
		for containerName, networks := range containerNetworkMap {
			hasUnreachableNetwork := false
			var unreachableNetworkNames []string

			for _, networkName := range networks {
				// We need to check if this network name corresponds to an unreachable network
				// Since we only have network IDs in unreachableNetworks, we'll need to find the network info
				networkList, err := apiClient.NetworkList(ctx, network.ListOptions{})
				if err == nil {
					for _, net := range networkList {
						if _, exists := unreachableNetworks[net.ID]; exists && net.Name == networkName {
							hasUnreachableNetwork = true
							unreachableNetworkNames = append(unreachableNetworkNames, networkName)
						}
					}
				}
			}

			if hasUnreachableNetwork {
				fmt.Printf("  - Container: %s on unreachable networks: %v\n", containerName, unreachableNetworkNames)
			}
		}

		fmt.Printf("  These services may not be accessible through Traefik reverse proxy.\n")
		fmt.Printf("  Consider connecting Traefik to these networks or moving services to accessible networks.\n")
	}

	return nil
}
