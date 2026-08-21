package h20

import (
	"bytes"
	"errors"
	"testing"
	"time"

	"github.com/ethereum-optimism/optimism/op-devstack/devtest"
	"github.com/ethereum-optimism/optimism/op-devstack/dsl"
	"github.com/ethereum-optimism/optimism/op-devstack/presets"
	"github.com/ethereum-optimism/optimism/op-devstack/sysgo"
	"github.com/ethereum-optimism/optimism/op-service/eth"
	"github.com/ethereum-optimism/optimism/op-service/txplan"
	"github.com/ethereum/go-ethereum"
	"github.com/ethereum/go-ethereum/accounts/abi"
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/common/hexutil"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/rpc"
	"github.com/lmittmann/w3"
)

var (
	factoryAddress            = common.HexToAddress("0x0177FF0000000000000000000000000000000000")
	activationRegistryAddress = common.HexToAddress("0x0177FF0000000000000000000000000000000001")
	policyRegistryAddress     = common.HexToAddress("0x0177FF0000000000000000000000000000000002")

	policyRegistryFeature = common.HexToHash("0xb582ebae03f16fee49a6763f78df482fb11ae73f103ed0d330bbe556aa90a43f")
	stablecoinFeature     = common.HexToHash("0xecfa0def2c10020caaf65e6155aa69c84b24892aaef76eeac52e0e2b3a0b8601")
	assetFeature          = common.HexToHash("0xcdcc772fe4cbdb1029f822861176d09e646db96723d4c1e82ddfdeb8163ef54c")
	coreStorageRoot       = common.HexToHash("0xc78b71fee795ddd74aff64ea9b2474194c938c3196430e10bb5f01ed48434000")

	adminFn       = w3.MustNewFunc("admin()", "address")
	activateFn    = w3.MustNewFunc("activate(bytes32)", "")
	isActivatedFn = w3.MustNewFunc("isActivated(bytes32)", "bool")
	getAddressFn  = w3.MustNewFunc("getH20Address(uint8,address,bytes32)", "address")
	createH20Fn   = w3.MustNewFunc("createH20(uint8,bytes32,bytes,bytes[])", "address")
)

const h20ActivationOffset = uint64(6)

type h20System struct {
	seq      *dsl.L2ELNode
	verifier *dsl.L2ELNode
	admin    *dsl.EOA
	genesis  *types.Header
}

type assetCreateParams struct {
	Version      uint8
	Name         string
	Symbol       string
	InitialAdmin common.Address
	Decimals     uint8
}

type stablecoinCreateParams struct {
	Version      uint8
	Name         string
	Symbol       string
	InitialAdmin common.Address
	Currency     string
}

func encodeAssetCreateParams(t devtest.T, params assetCreateParams) []byte {
	tuple, err := abi.NewType("tuple", "", []abi.ArgumentMarshaling{
		{Name: "version", Type: "uint8"},
		{Name: "name", Type: "string"},
		{Name: "symbol", Type: "string"},
		{Name: "initialAdmin", Type: "address"},
		{Name: "decimals", Type: "uint8"},
	})
	t.Require().NoError(err)
	out, err := (abi.Arguments{{Type: tuple}}).Pack(params)
	t.Require().NoError(err)
	return out
}

func encodeStablecoinCreateParams(t devtest.T, params stablecoinCreateParams) []byte {
	tuple, err := abi.NewType("tuple", "", []abi.ArgumentMarshaling{
		{Name: "version", Type: "uint8"},
		{Name: "name", Type: "string"},
		{Name: "symbol", Type: "string"},
		{Name: "initialAdmin", Type: "address"},
		{Name: "currency", Type: "string"},
	})
	t.Require().NoError(err)
	out, err := (abi.Arguments{{Type: tuple}}).Pack(params)
	t.Require().NoError(err)
	return out
}

func newH20System(t devtest.T) *h20System {
	wallet := dsl.NewRandomHDWallet(t, 40)
	adminKey := wallet.NewKey()
	offset := h20ActivationOffset
	clKind := sysgo.ResolveMixedL2CLKind()
	runtime := sysgo.NewMixedSingleChainRuntime(t, sysgo.MixedSingleChainPresetConfig{
		NodeSpecs: []sysgo.MixedSingleChainNodeSpec{
			{ELKey: "sequencer-op-reth", CLKey: "sequencer", ELKind: sysgo.MixedL2ELOpReth, CLKind: clKind, IsSequencer: true},
			{ELKey: "verifier-op-reth", CLKey: "verifier", ELKind: sysgo.MixedL2ELOpReth, CLKind: clKind},
		},
		H20ActivationOffset: &offset,
		H20ActivationAdmin:  adminKey.Address(),
	})
	frontends := presets.NewMixedSingleChainFrontends(t, runtime)
	seq := frontends.L2Network.PrimaryEL()
	var verifier *dsl.L2ELNode
	for _, node := range frontends.Nodes {
		if !node.Spec.IsSequencer {
			verifier = node.EL
		}
	}
	t.Require().NotNil(verifier)
	admin := adminKey.User(seq)
	frontends.FaucetL2.Fund(admin.Address(), eth.OneEther)
	admin.WaitForBalanceAtLeast(eth.OneEther)
	genesis, err := seq.Escape().L2EthClient().HeaderByNumber(t.Ctx(), 0)
	t.Require().NoError(err)
	return &h20System{seq: seq, verifier: verifier, admin: admin, genesis: genesis}
}

func codeAt(t devtest.T, node *dsl.L2ELNode, address common.Address, blockHash common.Hash) []byte {
	code, err := node.Escape().L2EthClient().CodeAtHash(t.Ctx(), address, blockHash)
	t.Require().NoError(err)
	return code
}

func latestHeader(t devtest.T, node *dsl.L2ELNode) *types.Header {
	header, err := node.Escape().L2EthClient().HeaderByLabel(t.Ctx(), eth.Unsafe)
	t.Require().NoError(err)
	return header
}

func call(t devtest.T, node *dsl.L2ELNode, from common.Address, to common.Address, data []byte) []byte {
	out, err := node.Escape().L2EthClient().Call(t.Ctx(), ethereum.CallMsg{From: from, To: &to, Data: data}, rpc.LatestBlockNumber)
	t.Require().NoError(err)
	return out
}

func transact(t devtest.T, from *dsl.EOA, to common.Address, data []byte) *types.Receipt {
	tx := from.Transact(from.Plan(), txplan.WithTo(&to), txplan.WithData(data))
	receipt, err := tx.Included.Eval(t.Ctx())
	t.Require().NoError(err)
	t.Require().Equal(uint64(types.ReceiptStatusSuccessful), receipt.Status)
	return receipt
}

func waitForHeader(t devtest.T, node *dsl.L2ELNode, number uint64, hash common.Hash) *types.Header {
	var header *types.Header
	t.Require().Eventually(func() bool {
		var err error
		header, err = node.Escape().L2EthClient().HeaderByNumber(t.Ctx(), number)
		return err == nil && header.Hash() == hash
	}, 30*time.Second, 250*time.Millisecond)
	return header
}

func revertData(err error) []byte {
	var dataErr rpc.DataError
	if !errors.As(err, &dataErr) {
		return nil
	}
	switch value := dataErr.ErrorData().(type) {
	case string:
		return common.FromHex(value)
	case []byte:
		return value
	default:
		return nil
	}
}

func TestH20ActivationFactoryAndCrossNodeState(t_ *testing.T) {
	t := devtest.SerialT(t_)
	sys := newH20System(t)

	for _, singleton := range []common.Address{factoryAddress, activationRegistryAddress, policyRegistryAddress} {
		t.Require().Empty(codeAt(t, sys.seq, singleton, sys.genesis.Hash()), "singleton %s must not exist before H20", singleton)
	}
	adminCall, err := adminFn.EncodeArgs()
	t.Require().NoError(err)
	preActivationOut, err := sys.seq.Escape().L2EthClient().Call(t.Ctx(), ethereum.CallMsg{
		From: sys.admin.Address(), To: &activationRegistryAddress, Data: adminCall,
	}, rpc.BlockNumber(sys.genesis.Number.Uint64()))
	t.Require().NoError(err)
	t.Require().Empty(preActivationOut, "inactive native precompile must not dispatch calls")

	activationTime := sys.genesis.Time + h20ActivationOffset
	t.Require().Eventually(func() bool {
		return latestHeader(t, sys.seq).Time >= activationTime
	}, 30*time.Second, 250*time.Millisecond)
	postActivation := latestHeader(t, sys.seq)
	for _, singleton := range []common.Address{factoryAddress, activationRegistryAddress, policyRegistryAddress} {
		// Native singleton precompiles intentionally have no EVM bytecode. Activation
		// is proven by successful ABI dispatch below, not by eth_getCode.
		t.Require().Empty(codeAt(t, sys.seq, singleton, postActivation.Hash()))
	}

	adminOut := call(t, sys.seq, sys.admin.Address(), activationRegistryAddress, adminCall)
	var configuredAdmin common.Address
	t.Require().NoError(adminFn.DecodeReturns(adminOut, &configuredAdmin))
	t.Require().Equal(sys.admin.Address(), configuredAdmin)

	for _, feature := range []common.Hash{policyRegistryFeature, assetFeature, stablecoinFeature} {
		data, err := activateFn.EncodeArgs(feature)
		t.Require().NoError(err)
		receipt := transact(t, sys.admin, activationRegistryAddress, data)
		t.Require().NotEmpty(receipt.Logs, "activation must emit a log")
		check, err := isActivatedFn.EncodeArgs(feature)
		t.Require().NoError(err)
		out := call(t, sys.seq, sys.admin.Address(), activationRegistryAddress, check)
		var active bool
		t.Require().NoError(isActivatedFn.DecodeReturns(out, &active))
		t.Require().True(active)
	}

	salt := common.HexToHash("0x123456789abcdef123")
	getAddressData, err := getAddressFn.EncodeArgs(uint8(0), sys.admin.Address(), salt)
	t.Require().NoError(err)
	predictedOut := call(t, sys.seq, sys.admin.Address(), factoryAddress, getAddressData)
	var token common.Address
	t.Require().NoError(getAddressFn.DecodeReturns(predictedOut, &token))
	t.Require().Equal([]byte{0x01, 0x77}, token.Bytes()[:2])
	t.Require().Equal(byte(0), token.Bytes()[10], "asset variant byte")

	encodedParams := encodeAssetCreateParams(t, assetCreateParams{
		Version: 1, Name: "HSK Asset", Symbol: "H20A", InitialAdmin: sys.admin.Address(), Decimals: 18,
	})
	createData, err := createH20Fn.EncodeArgs(uint8(0), salt, encodedParams, [][]byte{})
	t.Require().NoError(err)

	createOut := call(t, sys.seq, sys.admin.Address(), factoryAddress, createData)
	var callToken common.Address
	t.Require().NoError(createH20Fn.DecodeReturns(createOut, &callToken))
	t.Require().Equal(token, callToken)

	gas, err := sys.seq.Escape().L2EthClient().EstimateGas(t.Ctx(), ethereum.CallMsg{From: sys.admin.Address(), To: &factoryAddress, Data: createData})
	t.Require().NoError(err)
	t.Require().Greater(gas, uint64(0))

	var trace map[string]any
	err = sys.seq.EthClient().RPC().CallContext(t.Ctx(), &trace, "debug_traceCall", map[string]any{
		"from": sys.admin.Address(), "to": factoryAddress, "data": hexutil.Bytes(createData),
	}, "latest", map[string]any{})
	t.Require().NoError(err)
	t.Require().NotEmpty(trace)

	createReceipt := transact(t, sys.admin, factoryAddress, createData)
	t.Require().NotEmpty(createReceipt.Logs, "H20 creation must emit logs")
	seqHeader, err := sys.seq.Escape().L2EthClient().HeaderByHash(t.Ctx(), createReceipt.BlockHash)
	t.Require().NoError(err)
	verifierHeader := waitForHeader(t, sys.verifier, createReceipt.BlockNumber.Uint64(), createReceipt.BlockHash)
	t.Require().Equal(seqHeader.Root, verifierHeader.Root)
	t.Require().Equal(seqHeader.ReceiptHash, verifierHeader.ReceiptHash)

	verifierReceipt, err := sys.verifier.Escape().L2EthClient().TransactionReceipt(t.Ctx(), createReceipt.TxHash)
	t.Require().NoError(err)
	t.Require().Equal(createReceipt.Status, verifierReceipt.Status)
	t.Require().Equal(createReceipt.GasUsed, verifierReceipt.GasUsed)
	t.Require().Equal(createReceipt.Logs, verifierReceipt.Logs)

	seqCode := codeAt(t, sys.seq, token, createReceipt.BlockHash)
	verifierCode := codeAt(t, sys.verifier, token, createReceipt.BlockHash)
	t.Require().NotEmpty(seqCode)
	t.Require().True(bytes.Equal(seqCode, verifierCode))
	seqStorage, err := sys.seq.Escape().L2EthClient().GetStorageAt(t.Ctx(), token, coreStorageRoot, createReceipt.BlockHash.Hex())
	t.Require().NoError(err)
	verifierStorage, err := sys.verifier.Escape().L2EthClient().GetStorageAt(t.Ctx(), token, coreStorageRoot, createReceipt.BlockHash.Hex())
	t.Require().NoError(err)
	t.Require().NotEqual(common.Hash{}, seqStorage)
	t.Require().Equal(seqStorage, verifierStorage)

	_, err = sys.seq.Escape().L2EthClient().Call(t.Ctx(), ethereum.CallMsg{From: sys.admin.Address(), To: &factoryAddress, Data: createData}, rpc.LatestBlockNumber)
	t.Require().Error(err)
	selector := crypto.Keccak256([]byte("TokenAlreadyExists(address)"))[:4]
	data := revertData(err)
	t.Require().GreaterOrEqual(len(data), 4, "missing duplicate-token revert data: %v", err)
	t.Require().Equal(selector, data[:4])

	stableSalt := common.HexToHash("0xabcdef123456789abcdef123")
	stableAddressData, err := getAddressFn.EncodeArgs(uint8(1), sys.admin.Address(), stableSalt)
	t.Require().NoError(err)
	stableAddressOut := call(t, sys.seq, sys.admin.Address(), factoryAddress, stableAddressData)
	var stableToken common.Address
	t.Require().NoError(getAddressFn.DecodeReturns(stableAddressOut, &stableToken))
	t.Require().Equal([]byte{0x01, 0x77}, stableToken.Bytes()[:2])
	t.Require().Equal(byte(1), stableToken.Bytes()[10], "stablecoin variant byte")
	stableParams := encodeStablecoinCreateParams(t, stablecoinCreateParams{
		Version: 1, Name: "HSK USD", Symbol: "HUSD", InitialAdmin: sys.admin.Address(), Currency: "USD",
	})
	stableCreateData, err := createH20Fn.EncodeArgs(uint8(1), stableSalt, stableParams, [][]byte{})
	t.Require().NoError(err)
	stableReceipt := transact(t, sys.admin, factoryAddress, stableCreateData)
	t.Require().NotEmpty(stableReceipt.Logs)
	stableVerifierHeader := waitForHeader(t, sys.verifier, stableReceipt.BlockNumber.Uint64(), stableReceipt.BlockHash)
	stableSeqHeader, err := sys.seq.Escape().L2EthClient().HeaderByHash(t.Ctx(), stableReceipt.BlockHash)
	t.Require().NoError(err)
	t.Require().Equal(stableSeqHeader.Root, stableVerifierHeader.Root)
	t.Require().Equal(stableSeqHeader.ReceiptHash, stableVerifierHeader.ReceiptHash)
	t.Require().NotEmpty(codeAt(t, sys.seq, stableToken, stableReceipt.BlockHash))
	t.Require().Equal(
		codeAt(t, sys.seq, stableToken, stableReceipt.BlockHash),
		codeAt(t, sys.verifier, stableToken, stableReceipt.BlockHash),
	)

	t.Logf("H20 parity block=%d hash=%s state_root=%s receipts_root=%s gas_used=%d storage=%s asset=%s stablecoin=%s stablecoin_block=%d trace_keys=%d",
		createReceipt.BlockNumber.Uint64(), createReceipt.BlockHash, seqHeader.Root, seqHeader.ReceiptHash,
		createReceipt.GasUsed, seqStorage, token, stableToken, stableReceipt.BlockNumber.Uint64(), len(trace))
}

func TestPreActivationOpGethSequencerOpRethVerifier(t_ *testing.T) {
	t := devtest.SerialT(t_)
	runtime := sysgo.NewMixedSingleChainRuntime(t, sysgo.MixedSingleChainPresetConfig{
		NodeSpecs: []sysgo.MixedSingleChainNodeSpec{
			{ELKey: "sequencer-op-geth", CLKey: "sequencer", ELKind: sysgo.MixedL2ELOpGeth, CLKind: sysgo.MixedL2CLOpNode, IsSequencer: true},
			{ELKey: "verifier-op-reth", CLKey: "verifier", ELKind: sysgo.MixedL2ELOpReth, CLKind: sysgo.MixedL2CLOpNode},
		},
		// op-geth is deprecated at Karst. Keep this compatibility baseline on
		// Jovian, the last fork implemented by both execution clients.
		DeployerOptions: []sysgo.DeployerOption{sysgo.WithKarstAtOffset(nil)},
	})
	frontends := presets.NewMixedSingleChainFrontends(t, runtime)
	seq := frontends.L2Network.PrimaryEL()
	var verifier *dsl.L2ELNode
	for _, node := range frontends.Nodes {
		if !node.Spec.IsSequencer {
			verifier = node.EL
		}
	}
	t.Require().NotNil(verifier)
	var seqHeader *types.Header
	t.Require().Eventually(func() bool {
		seqHeader = latestHeader(t, seq)
		return seqHeader.Number.Uint64() >= 3
	}, 30*time.Second, 250*time.Millisecond)
	verifierHeader := waitForHeader(t, verifier, seqHeader.Number.Uint64(), seqHeader.Hash())
	t.Require().Equal(seqHeader.Root, verifierHeader.Root)
	t.Require().Equal(seqHeader.ReceiptHash, verifierHeader.ReceiptHash)
	t.Require().Equal(seqHeader.GasUsed, verifierHeader.GasUsed)
	t.Require().Empty(codeAt(t, seq, factoryAddress, seqHeader.Hash()))
	adminCall, err := adminFn.EncodeArgs()
	t.Require().NoError(err)
	preH20Out, err := seq.Escape().L2EthClient().Call(t.Ctx(), ethereum.CallMsg{To: &activationRegistryAddress, Data: adminCall}, rpc.BlockNumber(seqHeader.Number.Uint64()))
	t.Require().NoError(err)
	t.Require().Empty(preH20Out)
	t.Logf("pre-H20 op-geth/op-reth parity block=%d hash=%s state_root=%s receipts_root=%s gas_used=%d",
		seqHeader.Number.Uint64(), seqHeader.Hash(), seqHeader.Root, seqHeader.ReceiptHash, seqHeader.GasUsed)
}
