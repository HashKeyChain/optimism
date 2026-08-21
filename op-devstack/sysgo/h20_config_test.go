package sysgo

import (
	"encoding/json"
	"testing"

	"github.com/ethereum/go-ethereum/common"
	"github.com/stretchr/testify/require"
)

func TestInjectH20Config(t *testing.T) {
	t.Setenv(h20TimeEnv, "12345")
	t.Setenv(h20AdminEnv, "0xcb00000000000000000000000000000000000000")

	genesis, err := injectH20Config([]byte(`{"config":{"chainId":901}}`), true)
	require.NoError(t, err)
	var gotGenesis map[string]any
	require.NoError(t, json.Unmarshal(genesis, &gotGenesis))
	config := gotGenesis["config"].(map[string]any)
	require.Equal(t, float64(12345), config["h20Time"])
	require.Equal(t, "0xCB00000000000000000000000000000000000000", config["h20ActivationAdmin"])

	rollup, err := injectH20Config([]byte(`{"l2_chain_id":901}`), false)
	require.NoError(t, err)
	var gotRollup map[string]any
	require.NoError(t, json.Unmarshal(rollup, &gotRollup))
	require.Equal(t, float64(12345), gotRollup["h20_time"])
	require.Equal(t, "0xCB00000000000000000000000000000000000000", gotRollup["h20_activation_admin"])
}

func TestInjectH20ConfigRejectsPartialAndInvalidValues(t *testing.T) {
	t.Setenv(h20TimeEnv, "1")
	_, err := injectH20Config([]byte(`{}`), false)
	require.ErrorContains(t, err, "must be configured together")

	t.Setenv(h20AdminEnv, "0x0000000000000000000000000000000000000000")
	_, err = injectH20Config([]byte(`{}`), false)
	require.ErrorContains(t, err, "non-zero address")
}

func TestInjectH20ConfigIgnoresLegacyB20Environment(t *testing.T) {
	t.Setenv("DEVSTACK_B20_TIME", "12345")
	t.Setenv("DEVSTACK_B20_ACTIVATION_ADMIN", "0xcb00000000000000000000000000000000000000")

	input := []byte(`{"l2_chain_id":901}`)
	output, err := injectH20Config(input, false)
	require.NoError(t, err)
	require.Equal(t, input, output)
}

func TestInjectH20ConfigDisabledReturnsInputUnchanged(t *testing.T) {
	input := []byte(`{"l2_chain_id":901}`)
	output, err := injectH20Config(input, false)
	require.NoError(t, err)
	require.Equal(t, input, output)
}

func TestInjectH20ConfigRejectsInvalidTimestamp(t *testing.T) {
	t.Setenv(h20TimeEnv, "not-a-timestamp")
	t.Setenv(h20AdminEnv, "0xcb00000000000000000000000000000000000000")

	_, err := injectH20Config([]byte(`{}`), false)
	require.ErrorContains(t, err, "invalid DEVSTACK_H20_TIME")
}

func TestInjectH20ConfigOverrideIsDeterministic(t *testing.T) {
	t.Setenv(h20TimeEnv, "99999")
	t.Setenv(h20AdminEnv, "0x9999999999999999999999999999999999999999")

	override := &h20RuntimeConfig{
		timestamp: 12345,
		admin:     common.HexToAddress("0x1111111111111111111111111111111111111111"),
	}
	rollup, err := injectH20ConfigWithOverride([]byte(`{"l2_chain_id":901}`), false, override)
	require.NoError(t, err)

	var got map[string]any
	require.NoError(t, json.Unmarshal(rollup, &got))
	require.Equal(t, float64(12345), got["h20_time"])
	require.Equal(t, override.admin.Hex(), got["h20_activation_admin"])
}
