package tcpproxy

import (
	"io"
	"net"
	"testing"
	"time"

	"github.com/ethereum/go-ethereum/log"
	"github.com/stretchr/testify/require"
)

func TestDisconnectAllKeepsListenerAvailable(t *testing.T) {
	upstream, err := net.Listen("tcp", "127.0.0.1:0")
	require.NoError(t, err)
	t.Cleanup(func() { _ = upstream.Close() })
	go func() {
		for {
			conn, err := upstream.Accept()
			if err != nil {
				return
			}
			go func() {
				defer conn.Close()
				_, _ = io.Copy(conn, conn)
			}()
		}
	}()

	proxy := New(log.New())
	require.NoError(t, proxy.Start())
	t.Cleanup(func() { _ = proxy.Close() })
	proxy.SetUpstream(upstream.Addr().String())

	conn, err := net.Dial("tcp", proxy.Addr())
	require.NoError(t, err)
	defer conn.Close()
	require.NoError(t, conn.SetDeadline(time.Now().Add(2*time.Second)))
	_, err = conn.Write([]byte("ok"))
	require.NoError(t, err)
	buf := make([]byte, 2)
	_, err = io.ReadFull(conn, buf)
	require.NoError(t, err)
	require.Equal(t, []byte("ok"), buf)

	proxy.DisconnectAll()
	_, err = conn.Read(buf)
	require.Error(t, err)
	require.NotEmpty(t, proxy.Addr(), "listener must remain available after disconnect")
}
