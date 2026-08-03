package sysgo

import (
	"os"
	"path/filepath"
	"syscall"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestCopyTreePreservesSparseFiles(t *testing.T) {
	src := t.TempDir()
	dst := filepath.Join(t.TempDir(), "snapshot")
	path := filepath.Join(src, "mdbx.dat")
	f, err := os.Create(path)
	require.NoError(t, err)
	require.NoError(t, f.Truncate(1<<30))
	require.NoError(t, f.Close())

	require.NoError(t, copyTree(src, dst))
	copied := filepath.Join(dst, "mdbx.dat")
	info, err := os.Stat(copied)
	require.NoError(t, err)
	require.Equal(t, int64(1<<30), info.Size())

	stat, ok := info.Sys().(*syscall.Stat_t)
	require.True(t, ok)
	require.Less(t, stat.Blocks*512, int64(1<<20), "sparse copy unexpectedly allocated the full file")
}
