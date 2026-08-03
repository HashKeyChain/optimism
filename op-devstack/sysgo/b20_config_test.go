package sysgo

import (
	"encoding/json"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestInjectB20Config(t *testing.T) {
	t.Setenv(b20TimeEnv, "12345")
	t.Setenv(b20AdminEnv, "0xcb00000000000000000000000000000000000000")

	genesis, err := injectB20Config([]byte(`{"config":{"chainId":901}}`), true)
	require.NoError(t, err)
	var gotGenesis map[string]any
	require.NoError(t, json.Unmarshal(genesis, &gotGenesis))
	config := gotGenesis["config"].(map[string]any)
	require.Equal(t, float64(12345), config["b20Time"])
	require.Equal(t, "0xCB00000000000000000000000000000000000000", config["b20ActivationAdmin"])

	rollup, err := injectB20Config([]byte(`{"l2_chain_id":901}`), false)
	require.NoError(t, err)
	var gotRollup map[string]any
	require.NoError(t, json.Unmarshal(rollup, &gotRollup))
	require.Equal(t, float64(12345), gotRollup["b20_time"])
	require.Equal(t, "0xCB00000000000000000000000000000000000000", gotRollup["b20_activation_admin"])
}

func TestInjectB20ConfigRejectsPartialAndInvalidValues(t *testing.T) {
	t.Setenv(b20TimeEnv, "1")
	_, err := injectB20Config([]byte(`{}`), false)
	require.ErrorContains(t, err, "must be configured together")

	t.Setenv(b20AdminEnv, "0x0000000000000000000000000000000000000000")
	_, err = injectB20Config([]byte(`{}`), false)
	require.ErrorContains(t, err, "non-zero address")
}
