import { writeFileSync } from "node:fs";

const policy =
  "Public guardian coverage only. CYPHES does not submit external reports in auto mode. Human approval is required before disclosure, escalation, payout claim, protocol contact, or any production interaction.";

const sourceMix = ["manual-curated", "github", "defillama"];

const protocols = [
  ["Uniswap", "DEX", ["Ethereum"], 1, "https://github.com/Uniswap/v2-core", ["contracts/UniswapV2Pair.sol", "contracts/UniswapV2ERC20.sol"], "https://docs.uniswap.org/", "https://github.com/Uniswap/v2-core"],
  ["Uniswap V3", "DEX", ["Ethereum"], 2, "https://github.com/Uniswap/v3-core", ["contracts/UniswapV3Pool.sol", "contracts/UniswapV3Factory.sol"], "https://docs.uniswap.org/", "https://github.com/Uniswap/v3-core"],
  ["Aave V3", "Lending", ["Ethereum", "Arbitrum", "Optimism", "Base", "Polygon"], 3, "https://github.com/aave/aave-v3-core", ["contracts/protocol/pool/Pool.sol", "contracts/protocol/libraries/logic/ValidationLogic.sol"], "https://aave.com/docs", "https://github.com/aave/aave-v3-core"],
  ["Compound V3", "Lending", ["Ethereum", "Base", "Arbitrum"], 4, "https://github.com/compound-finance/comet", ["contracts/Comet.sol", "contracts/CometRewards.sol"], "https://docs.compound.finance/", "https://github.com/compound-finance/comet"],
  ["Compound V2", "Lending", ["Ethereum"], 5, "https://github.com/compound-finance/compound-protocol", ["contracts/CToken.sol", "contracts/Comptroller.sol"], "https://docs.compound.finance/v2/", "https://github.com/compound-finance/compound-protocol"],
  ["MakerDAO DSS", "CDP", ["Ethereum"], 6, "https://github.com/makerdao/dss", ["src/vat.sol", "src/jug.sol"], "https://docs.makerdao.com/", "https://github.com/makerdao/dss"],
  ["Balancer V2", "DEX", ["Ethereum", "Arbitrum", "Polygon", "Base"], 7, "https://github.com/balancer/balancer-v2-monorepo", ["pkg/vault/contracts/Vault.sol", "pkg/pool-weighted/contracts/WeightedPool.sol"], "https://docs.balancer.fi/", "https://github.com/balancer/balancer-v2-monorepo"],
  ["Curve", "DEX", ["Ethereum"], 8, "https://github.com/curvefi/curve-contract", ["contracts/pools/3pool/StableSwap3Pool.vy"], "https://resources.curve.fi/", "https://github.com/curvefi/curve-contract"],
  ["Lido", "Liquid Staking", ["Ethereum"], 9, "https://github.com/lidofinance/core", ["contracts/0.4.24/Lido.sol", "contracts/0.8.9/StETH.sol"], "https://docs.lido.fi/", "https://github.com/lidofinance/core"],
  ["Rocket Pool", "Liquid Staking", ["Ethereum"], 10, "https://github.com/rocket-pool/rocketpool", ["contracts/contract/RocketDepositPool.sol", "contracts/contract/RocketMinipoolManager.sol"], "https://docs.rocketpool.net/", "https://github.com/rocket-pool/rocketpool"],
  ["Synthetix", "Derivatives", ["Ethereum", "Optimism"], 11, "https://github.com/Synthetixio/synthetix", ["contracts/Synthetix.sol", "contracts/Issuer.sol"], "https://docs.synthetix.io/", "https://github.com/Synthetixio/synthetix"],
  ["Frax", "Stablecoin", ["Ethereum", "Fraxtal"], 12, "https://github.com/FraxFinance/frax-solidity", ["src/contracts/Frax/Frax.sol", "src/contracts/Curve/CurveAMO.sol"], "https://docs.frax.finance/", "https://github.com/FraxFinance/frax-solidity"],
  ["Yearn", "Yield", ["Ethereum"], 13, "https://github.com/yearn/yearn-vaults", ["contracts/Vault.vy", "contracts/Registry.vy"], "https://docs.yearn.fi/", "https://github.com/yearn/yearn-vaults"],
  ["Convex", "Yield", ["Ethereum"], 14, "https://github.com/convex-eth/platform", ["contracts/contracts/Booster.sol", "contracts/contracts/BaseRewardPool.sol"], "https://docs.convexfinance.com/", "https://github.com/convex-eth/platform"],
  ["Sushi", "DEX", ["Ethereum", "Arbitrum", "Polygon"], 15, "https://github.com/sushiswap/sushiswap", ["protocols/sushiswap/contracts/UniswapV2Pair.sol"], "https://docs.sushi.com/", "https://github.com/sushiswap/sushiswap"],
  ["PancakeSwap V3", "DEX", ["BNB Chain", "Ethereum", "Arbitrum"], 16, "https://github.com/pancakeswap/pancake-v3-contracts", ["projects/v3-core/contracts/PancakeV3Pool.sol", "projects/v3-periphery/contracts/SwapRouter.sol"], "https://developer.pancakeswap.finance/", "https://github.com/pancakeswap/pancake-v3-contracts"],
  ["1inch Limit Order", "DEX Aggregator", ["Ethereum"], 17, "https://github.com/1inch/limit-order-protocol", ["contracts/LimitOrderProtocol.sol"], "https://docs.1inch.io/", "https://github.com/1inch/limit-order-protocol"],
  ["0x Protocol", "DEX Aggregator", ["Ethereum"], 18, "https://github.com/0xProject/protocol", ["contracts/zero-ex/contracts/src/ZeroEx.sol", "contracts/zero-ex/contracts/src/features/TransformERC20Feature.sol"], "https://docs.0xprotocol.org/", "https://github.com/0xProject/protocol"],
  ["GMX", "Derivatives", ["Arbitrum", "Avalanche"], 19, "https://github.com/gmx-io/gmx-contracts", ["contracts/core/Vault.sol", "contracts/core/Router.sol"], "https://docs.gmx.io/", "https://github.com/gmx-io/gmx-contracts"],
  ["dYdX Chain", "Derivatives", ["Cosmos"], 20, "https://github.com/dydxprotocol/v4-chain", ["protocol/x/clob", "protocol/x/perpetuals"], "https://docs.dydx.xyz/", "https://github.com/dydxprotocol/v4-chain"],
  ["EigenLayer", "Restaking", ["Ethereum"], 21, "https://github.com/Layr-Labs/eigenlayer-contracts", ["src/contracts/core/StrategyManager.sol", "src/contracts/core/DelegationManager.sol"], "https://docs.eigenlayer.xyz/", "https://github.com/Layr-Labs/eigenlayer-contracts"],
  ["OpenZeppelin Contracts", "Library", ["Ethereum"], 22, "https://github.com/OpenZeppelin/openzeppelin-contracts", ["contracts/token/ERC20/ERC20.sol", "contracts/access/AccessControl.sol"], "https://docs.openzeppelin.com/contracts/", "https://github.com/OpenZeppelin/openzeppelin-contracts/security"],
  ["Safe", "Smart Account", ["Ethereum", "Base", "Arbitrum", "Optimism"], 23, "https://github.com/safe-global/safe-smart-account", ["contracts/Safe.sol", "contracts/base/ModuleManager.sol"], "https://docs.safe.global/", "https://github.com/safe-global/safe-smart-account"],
  ["ENS", "Identity", ["Ethereum"], 24, "https://github.com/ensdomains/ens-contracts", ["contracts/registry/ENSRegistry.sol", "contracts/resolvers/PublicResolver.sol"], "https://docs.ens.domains/", "https://github.com/ensdomains/ens-contracts"],
  ["Chainlink", "Oracle", ["Ethereum", "Arbitrum", "Base"], 25, "https://github.com/smartcontractkit/chainlink", ["contracts/src/v0.8/automation", "contracts/src/v0.8/shared"], "https://docs.chain.link/", "https://github.com/smartcontractkit/chainlink"],
  ["The Graph", "Indexing", ["Ethereum"], 26, "https://github.com/graphprotocol/contracts", ["contracts/governance/Governed.sol", "contracts/staking/Staking.sol"], "https://thegraph.com/docs/", "https://github.com/graphprotocol/contracts"],
  ["UMA", "Oracle", ["Ethereum"], 27, "https://github.com/UMAprotocol/protocol", ["packages/core/contracts/oracle/implementation/OptimisticOracleV3.sol", "packages/core/contracts/data-verification-mechanism/implementation/Voting.sol"], "https://docs.uma.xyz/", "https://github.com/UMAprotocol/protocol"],
  ["Across", "Bridge", ["Ethereum", "Arbitrum", "Base", "Optimism"], 28, "https://github.com/across-protocol/contracts", ["contracts/SpokePool.sol", "contracts/HubPool.sol"], "https://docs.across.to/", "https://github.com/across-protocol/contracts"],
  ["Hop Protocol", "Bridge", ["Ethereum", "Arbitrum", "Optimism", "Polygon"], 29, "https://github.com/hop-protocol/contracts", ["contracts/bridges/L1_Bridge.sol", "contracts/bridges/L2_Bridge.sol"], "https://docs.hop.exchange/", "https://github.com/hop-protocol/contracts"],
  ["Pendle", "Yield", ["Ethereum", "Arbitrum"], 30, "https://github.com/pendle-finance/pendle-core-v2-public", ["contracts/core/Market/PendleMarket.sol", "contracts/core/StandardizedYield/PendleERC4626SY.sol"], "https://docs.pendle.finance/", "https://github.com/pendle-finance/pendle-core-v2-public"],
  ["Morpho Blue", "Lending", ["Ethereum", "Base"], 31, "https://github.com/morpho-org/morpho-blue", ["src/Morpho.sol", "src/libraries/MarketParamsLib.sol"], "https://docs.morpho.org/", "https://github.com/morpho-org/morpho-blue"],
  ["Liquity", "Stablecoin", ["Ethereum"], 32, "https://github.com/liquity/dev", ["packages/contracts/contracts/BorrowerOperations.sol", "packages/contracts/contracts/TroveManager.sol"], "https://docs.liquity.org/", "https://github.com/liquity/dev"],
  ["Euler", "Lending", ["Ethereum"], 33, "https://github.com/euler-xyz/euler-contracts", ["contracts/modules/eToken.sol", "contracts/modules/dToken.sol"], "https://docs.euler.finance/", "https://github.com/euler-xyz/euler-contracts"],
  ["Venus", "Lending", ["BNB Chain"], 34, "https://github.com/VenusProtocol/venus-protocol", ["contracts/Comptroller.sol", "contracts/Tokens/VToken.sol"], "https://docs-v4.venus.io/", "https://github.com/VenusProtocol/venus-protocol"],
  ["Stargate", "Bridge", ["Ethereum", "Arbitrum", "Optimism", "Avalanche"], 35, "https://github.com/stargate-protocol/stargate", ["contracts/Pool.sol", "contracts/Router.sol"], "https://stargateprotocol.gitbook.io/", "https://github.com/stargate-protocol/stargate"],
  ["Aragon OSx", "DAO", ["Ethereum"], 36, "https://github.com/aragon/osx", ["packages/contracts/src/core/dao/DAO.sol", "packages/contracts/src/framework/plugin/repo/PluginRepo.sol"], "https://devs.aragon.org/", "https://github.com/aragon/osx"],
  ["Gnosis Conditional Tokens", "Prediction Markets", ["Ethereum"], 37, "https://github.com/gnosis/conditional-tokens-contracts", ["contracts/ConditionalTokens.sol"], "https://gnosis-conditional-tokens.readthedocs.io/", "https://github.com/gnosis/conditional-tokens-contracts"],
  ["Superfluid", "Streaming Payments", ["Ethereum", "Polygon", "Optimism", "Base"], 38, "https://github.com/superfluid-finance/protocol-monorepo", ["packages/ethereum-contracts/contracts/superfluid/Superfluid.sol", "packages/ethereum-contracts/contracts/agreements/ConstantFlowAgreementV1.sol"], "https://docs.superfluid.finance/", "https://github.com/superfluid-finance/protocol-monorepo"],
  ["Seaport", "Marketplace", ["Ethereum"], 39, "https://github.com/ProjectOpenSea/seaport", ["contracts/Seaport.sol", "contracts/lib/OrderValidator.sol"], "https://docs.opensea.io/", "https://github.com/ProjectOpenSea/seaport"],
  ["LooksRare", "Marketplace", ["Ethereum"], 40, "https://github.com/LooksRare/contracts-exchange-v2", ["contracts/LooksRareProtocol.sol"], "https://docs.looksrare.org/", "https://github.com/LooksRare/contracts-exchange-v2"],
  ["Nouns DAO", "DAO", ["Ethereum"], 41, "https://github.com/nounsDAO/nouns-monorepo", ["packages/nouns-contracts/contracts/governance/NounsDAOLogicV3.sol", "packages/nouns-contracts/contracts/NounsToken.sol"], "https://docs.nouns.build/", "https://github.com/nounsDAO/nouns-monorepo"],
  ["Gitcoin Grants", "Public Goods", ["Ethereum"], 42, "https://github.com/gitcoinco/grants-stack", ["packages/contracts/contracts/allo/Allo.sol"], "https://docs.allo.gitcoin.co/", "https://github.com/gitcoinco/grants-stack"],
  ["Zora", "NFT", ["Ethereum", "Base"], 43, "https://github.com/ourzora/zora-protocol", ["packages/protocol-contracts/src/market/ZoraV3.sol", "packages/erc721-drop/src/ERC721Drop.sol"], "https://docs.zora.co/", "https://github.com/ourzora/zora-protocol"],
  ["Reservoir", "NFT Infrastructure", ["Ethereum"], 44, "https://github.com/reservoirprotocol/indexer", ["packages/indexer/src"], "https://docs.reservoir.tools/", "https://github.com/reservoirprotocol/indexer"],
  ["Account Abstraction", "Wallet Infrastructure", ["Ethereum"], 45, "https://github.com/eth-infinitism/account-abstraction", ["contracts/core/EntryPoint.sol", "contracts/core/StakeManager.sol"], "https://docs.erc4337.io/", "https://github.com/eth-infinitism/account-abstraction"],
  ["WETH10", "Token Primitive", ["Ethereum"], 46, "https://github.com/WETH10/WETH10", ["contracts/WETH10.sol"], "https://github.com/WETH10/WETH10", "https://github.com/WETH10/WETH10"],
  ["Solmate", "Library", ["Ethereum"], 47, "https://github.com/transmissions11/solmate", ["src/tokens/ERC20.sol", "src/auth/Owned.sol"], "https://github.com/transmissions11/solmate", "https://github.com/transmissions11/solmate"],
  ["Solady", "Library", ["Ethereum"], 48, "https://github.com/Vectorized/solady", ["src/tokens/ERC20.sol", "src/auth/Ownable.sol"], "https://github.com/Vectorized/solady", "https://github.com/Vectorized/solady"],
  ["PRBMath", "Library", ["Ethereum"], 49, "https://github.com/PaulRBerg/prb-math", ["src/SD59x18.sol", "src/UD60x18.sol"], "https://github.com/PaulRBerg/prb-math", "https://github.com/PaulRBerg/prb-math"],
  ["Solidity", "Compiler", ["Ethereum"], 50, "https://github.com/ethereum/solidity", ["libsolidity", "docs/security-considerations.rst"], "https://docs.soliditylang.org/", "https://github.com/ethereum/solidity/security"],
  ["Uniswap V4", "DEX", ["Ethereum"], 51, "https://github.com/Uniswap/v4-core", [], "https://docs.uniswap.org/", "https://github.com/Uniswap/v4-core/security"],
  ["Permit2", "Token Approval", ["Ethereum"], 52, "https://github.com/Uniswap/permit2", [], "https://docs.uniswap.org/contracts/permit2/overview", "https://github.com/Uniswap/permit2/security"],
  ["Universal Router", "DEX Aggregator", ["Ethereum"], 53, "https://github.com/Uniswap/universal-router", [], "https://docs.uniswap.org/contracts/universal-router/overview", "https://github.com/Uniswap/universal-router/security"],
  ["Aave V3 Periphery", "Lending", ["Ethereum", "Arbitrum", "Optimism", "Base", "Polygon"], 54, "https://github.com/aave/aave-v3-periphery", [], "https://aave.com/docs", "https://github.com/aave/aave-v3-periphery/security"],
  ["Balancer V3", "DEX", ["Ethereum", "Arbitrum", "Base", "Polygon"], 55, "https://github.com/balancer/balancer-v3-monorepo", [], "https://docs.balancer.fi/", "https://github.com/balancer/balancer-v3-monorepo/security"],
  ["CoW Protocol", "DEX Aggregator", ["Ethereum"], 56, "https://github.com/cowprotocol/contracts", [], "https://docs.cow.fi/", "https://github.com/cowprotocol/contracts/security"],
  ["Wormhole", "Bridge", ["Ethereum", "Solana", "Base", "Arbitrum"], 57, "https://github.com/wormhole-foundation/wormhole", [], "https://wormhole.com/docs/", "https://github.com/wormhole-foundation/wormhole/security"],
  ["Optimism Bedrock", "L2", ["Ethereum", "Optimism"], 58, "https://github.com/ethereum-optimism/optimism", [], "https://docs.optimism.io/", "https://github.com/ethereum-optimism/optimism/security"],
  ["Arbitrum Nitro", "L2", ["Ethereum", "Arbitrum"], 59, "https://github.com/OffchainLabs/nitro", [], "https://docs.arbitrum.io/", "https://github.com/OffchainLabs/nitro/security"],
  ["zkSync Era", "L2", ["Ethereum", "zkSync"], 60, "https://github.com/matter-labs/era-contracts", [], "https://docs.zksync.io/", "https://github.com/matter-labs/era-contracts/security"],
  ["Polygon zkEVM", "L2", ["Ethereum", "Polygon"], 61, "https://github.com/0xPolygonHermez/zkevm-contracts", [], "https://docs.polygon.technology/zkEVM/", "https://github.com/0xPolygonHermez/zkevm-contracts/security"],
  ["Axelar GMP", "Bridge", ["Ethereum", "Cosmos"], 62, "https://github.com/axelarnetwork/axelar-gmp-sdk-solidity", [], "https://docs.axelar.dev/", "https://github.com/axelarnetwork/axelar-gmp-sdk-solidity/security"],
  ["LayerZero V2", "Bridge", ["Ethereum", "Arbitrum", "Base", "Optimism"], 63, "https://github.com/LayerZero-Labs/LayerZero-v2", [], "https://docs.layerzero.network/", "https://github.com/LayerZero-Labs/LayerZero-v2/security"],
  ["Gearbox V3", "Credit", ["Ethereum"], 64, "https://github.com/Gearbox-protocol/core-v3", [], "https://docs.gearbox.fi/", "https://github.com/Gearbox-protocol/core-v3/security"],
  ["Sablier V2", "Streaming Payments", ["Ethereum"], 65, "https://github.com/sablier-labs/v2-core", [], "https://docs.sablier.com/", "https://github.com/sablier-labs/v2-core/security"],
  ["ENS Name Wrapper", "Identity", ["Ethereum"], 66, "https://github.com/ensdomains/name-wrapper", [], "https://docs.ens.domains/", "https://github.com/ensdomains/name-wrapper/security"],
  ["Safe Modules", "Smart Account", ["Ethereum", "Base", "Arbitrum", "Optimism"], 67, "https://github.com/safe-global/safe-modules", [], "https://docs.safe.global/", "https://github.com/safe-global/safe-modules/security"],
  ["Fraxlend", "Lending", ["Ethereum", "Fraxtal"], 68, "https://github.com/fraxfinance/fraxlend", [], "https://docs.frax.finance/", "https://github.com/fraxfinance/fraxlend/security"],
  ["Ribbon V2", "Options", ["Ethereum"], 69, "https://github.com/ribbon-finance/ribbon-v2", [], "https://docs.ribbon.finance/", "https://github.com/ribbon-finance/ribbon-v2/security"],
  ["PoolTogether V5", "Savings", ["Ethereum", "Optimism", "Base"], 70, "https://github.com/pooltogether/v5-prize-pool", [], "https://dev.pooltogether.com/", "https://github.com/pooltogether/v5-prize-pool/security"],
  ["Forge Std", "Testing Library", ["Ethereum"], 71, "https://github.com/foundry-rs/forge-std", [], "https://book.getfoundry.sh/forge/forge-std", "https://github.com/foundry-rs/forge-std/security"],
  ["Ethereum Execution Specs", "Protocol", ["Ethereum"], 72, "https://github.com/ethereum/execution-specs", [], "https://ethereum.github.io/execution-specs/", "https://github.com/ethereum/execution-specs/security"],
  ["Reth", "Execution Client", ["Ethereum"], 73, "https://github.com/paradigmxyz/reth", [], "https://reth.rs/", "https://github.com/paradigmxyz/reth/security"],
  // Current public-bounty repositories, used as a relevance guide for broad
  // guardian coverage. A repository appearing here is not a claim that every
  // file or impact is bounty-eligible; human triage still checks live scope.
  ["Euler Vault Kit", "Lending", ["Ethereum"], 74, "https://github.com/euler-xyz/euler-vault-kit", [], "https://docs.euler.finance/", "https://cantina.xyz/bounties/4d285eee-602e-440a-845e-25e155cec26a", ["cantina", "github", "bounty-guided"]],
  ["Euler EVC", "Lending Infrastructure", ["Ethereum"], 75, "https://github.com/euler-xyz/ethereum-vault-connector", [], "https://docs.euler.finance/", "https://cantina.xyz/bounties/4d285eee-602e-440a-845e-25e155cec26a", ["cantina", "github", "bounty-guided"]],
  ["Euler Price Oracle", "Oracle", ["Ethereum"], 76, "https://github.com/euler-xyz/euler-price-oracle", [], "https://docs.euler.finance/", "https://cantina.xyz/bounties/4d285eee-602e-440a-845e-25e155cec26a", ["cantina", "github", "bounty-guided"]],
  ["EulerSwap", "DEX", ["Ethereum"], 77, "https://github.com/euler-xyz/euler-swap", [], "https://docs.euler.finance/", "https://cantina.xyz/bounties/4d285eee-602e-440a-845e-25e155cec26a", ["cantina", "github", "bounty-guided"]],
  ["Euler Earn", "Yield", ["Ethereum"], 78, "https://github.com/euler-xyz/euler-earn", [], "https://docs.euler.finance/", "https://cantina.xyz/bounties/4d285eee-602e-440a-845e-25e155cec26a", ["cantina", "github", "bounty-guided"]],
  ["Euler Fee Flow", "Auction", ["Ethereum"], 79, "https://github.com/euler-xyz/fee-flow", [], "https://docs.euler.finance/", "https://cantina.xyz/bounties/4d285eee-602e-440a-845e-25e155cec26a", ["cantina", "github", "bounty-guided"]],
  ["Euler Reward Streams", "Rewards", ["Ethereum"], 80, "https://github.com/euler-xyz/reward-streams", [], "https://docs.euler.finance/", "https://cantina.xyz/bounties/4d285eee-602e-440a-845e-25e155cec26a", ["cantina", "github", "bounty-guided"]],
  ["MetaMorpho", "Lending", ["Ethereum", "Base"], 81, "https://github.com/morpho-org/metamorpho", [], "https://docs.morpho.org/", "https://cantina.xyz/bounties/35a5f0a1-2ffd-432c-8f3b-77d169add8c3", ["cantina", "github", "bounty-guided"]],
  ["Morpho Vault V2", "Lending", ["Ethereum", "Base"], 82, "https://github.com/morpho-org/vault-v2", [], "https://docs.morpho.org/", "https://cantina.xyz/bounties/35a5f0a1-2ffd-432c-8f3b-77d169add8c3", ["cantina", "github", "bounty-guided"]],
  ["Morpho Bundler3", "Lending Infrastructure", ["Ethereum", "Base"], 83, "https://github.com/morpho-org/bundler3", [], "https://docs.morpho.org/", "https://cantina.xyz/bounties/35a5f0a1-2ffd-432c-8f3b-77d169add8c3", ["cantina", "github", "bounty-guided"]],
  ["Morpho Pre-liquidation", "Liquidation", ["Ethereum", "Base"], 84, "https://github.com/morpho-org/pre-liquidation", [], "https://docs.morpho.org/", "https://cantina.xyz/bounties/35a5f0a1-2ffd-432c-8f3b-77d169add8c3", ["cantina", "github", "bounty-guided"]],
  ["Morpho Public Allocator", "Lending Infrastructure", ["Ethereum", "Base"], 85, "https://github.com/morpho-org/public-allocator", [], "https://docs.morpho.org/", "https://cantina.xyz/bounties/35a5f0a1-2ffd-432c-8f3b-77d169add8c3", ["cantina", "github", "bounty-guided"]],
  ["Morpho Blue Oracles", "Oracle", ["Ethereum", "Base"], 86, "https://github.com/morpho-org/morpho-blue-oracles", [], "https://docs.morpho.org/", "https://cantina.xyz/bounties/35a5f0a1-2ffd-432c-8f3b-77d169add8c3", ["cantina", "github", "bounty-guided"]],
  ["Reserve Protocol", "Asset-backed Token", ["Ethereum", "Base"], 87, "https://github.com/reserve-protocol/protocol", [], "https://reserve.org/protocol/", "https://cantina.xyz/bounties/3709ca85-4050-407e-9b36-51f5d5ea9b00", ["cantina", "github", "bounty-guided"]],
  ["Reserve Index DTF", "Index", ["Ethereum", "Base"], 88, "https://github.com/reserve-protocol/reserve-index-dtf", [], "https://reserve.org/protocol/", "https://cantina.xyz/bounties/3709ca85-4050-407e-9b36-51f5d5ea9b00", ["cantina", "github", "bounty-guided"]],
  ["Polymarket Contract Security", "Prediction Markets", ["Polygon"], 89, "https://github.com/Polymarket/contract-security", [], "https://docs.polymarket.com/", "https://cantina.xyz/bounties/ff945ca2-2a6e-4b83-b1b6-7a0cd3b94bea", ["cantina", "github", "bounty-guided"]],
  ["Polymarket CTF Exchange V2", "Prediction Markets", ["Polygon"], 90, "https://github.com/Polymarket/ctf-exchange-v2", [], "https://docs.polymarket.com/", "https://cantina.xyz/bounties/ff945ca2-2a6e-4b83-b1b6-7a0cd3b94bea", ["cantina", "github", "bounty-guided"]],
  ["Paxos PYUSD", "Stablecoin", ["Ethereum", "Solana"], 91, "https://github.com/paxosglobal/pyusd-contract", [], "https://docs.paxos.com/", "https://cantina.xyz/bounties/6a6ef71c-383d-4357-85c1-a0d1dbb6659b", ["cantina", "github", "bounty-guided"]],
  ["Paxos USDG", "Stablecoin", ["Ethereum"], 92, "https://github.com/paxosglobal/usdg-contract", [], "https://docs.paxos.com/", "https://cantina.xyz/bounties/6a6ef71c-383d-4357-85c1-a0d1dbb6659b", ["cantina", "github", "bounty-guided"]],
  ["Paxos Cross-chain", "Bridge", ["Ethereum", "Solana"], 93, "https://github.com/paxosglobal/cross-chain-contracts", [], "https://docs.paxos.com/", "https://cantina.xyz/bounties/6a6ef71c-383d-4357-85c1-a0d1dbb6659b", ["cantina", "github", "bounty-guided"]],
  ["UniswapX", "DEX Aggregator", ["Ethereum", "Unichain"], 94, "https://github.com/Uniswap/UniswapX", [], "https://docs.uniswap.org/", "https://cantina.xyz/bounties/f9df94db-c7b1-434b-bb06-d1360abdd1be", ["cantina", "github", "bounty-guided"]],
  ["Uniswap Protocol Fees", "DEX", ["Ethereum", "Unichain"], 95, "https://github.com/Uniswap/protocol-fees", [], "https://docs.uniswap.org/", "https://cantina.xyz/bounties/f9df94db-c7b1-434b-bb06-d1360abdd1be", ["cantina", "github", "bounty-guided"]],
  ["LI.FI Contracts", "Bridge Aggregator", ["Ethereum", "Arbitrum", "Base"], 96, "https://github.com/lifinance/contracts", [], "https://docs.li.fi/", "https://cantina.xyz/bounties/260585d8-a3e8-4d70-8077-b6f3f5f0391b", ["cantina", "github", "bounty-guided"]],
  ["Centrifuge Protocol", "RWA", ["Ethereum", "Base"], 97, "https://github.com/centrifuge/protocol", [], "https://docs.centrifuge.io/", "https://cantina.xyz/bounties/6cc9d51a-ac1e-4385-a88a-a3924e40c00e", ["cantina", "github", "bounty-guided"]],
  ["Alchemy Modular Account", "Smart Account", ["Ethereum", "Base"], 98, "https://github.com/alchemyplatform/modular-account", [], "https://accountkit.alchemy.com/", "https://cantina.xyz/bounties/246de4d3-e138-4340-bdfc-fc4c95951491", ["cantina", "github", "bounty-guided"]],
  ["Ethena Public Assets", "Stablecoin", ["Ethereum"], 99, "https://github.com/ethena-labs/bbp-public-assets", [], "https://docs.ethena.fi/", "https://immunefi.com/bug-bounty/ethena/information/", ["immunefi", "github", "bounty-guided"]],
  ["Ether.fi Smart Contracts", "Liquid Restaking", ["Ethereum"], 100, "https://github.com/etherfi-protocol/smart-contracts", [], "https://etherfi.gitbook.io/etherfi/", "https://immunefi.com/bug-bounty/etherfi/information/", ["immunefi", "github", "bounty-guided"]],
  ["Ether.fi Cash V3", "Smart Account", ["Ethereum"], 101, "https://github.com/etherfi-protocol/cash-v3", [], "https://etherfi.gitbook.io/etherfi/", "https://immunefi.com/bug-bounty/etherfi/information/", ["immunefi", "github", "bounty-guided"]],
  ["Babylon", "Bitcoin Staking", ["Bitcoin", "Cosmos"], 102, "https://github.com/babylonlabs-io/babylon", [], "https://docs.babylonlabs.io/", "https://immunefi.com/bug-bounty/babylon-labs/information/", ["immunefi", "github", "bounty-guided"]],
  ["Berachain Contracts", "L1", ["Berachain"], 103, "https://github.com/berachain/contracts", [], "https://docs.berachain.com/", "https://immunefi.com/bug-bounty/berachain/information/", ["immunefi", "github", "bounty-guided"]],
  ["Berachain Beacon Kit", "Consensus", ["Berachain"], 104, "https://github.com/berachain/beacon-kit", [], "https://docs.berachain.com/", "https://immunefi.com/bug-bounty/berachain/information/", ["immunefi", "github", "bounty-guided"]],
  ["deBridge", "Bridge", ["Ethereum", "Solana"], 105, "https://github.com/debridge-finance/debridge-contracts-v1", [], "https://docs.debridge.com/", "https://immunefi.com/bug-bounty/debridge/information/", ["immunefi", "github", "bounty-guided"]],
  ["Celer SGN V2", "Bridge", ["Ethereum", "BNB Chain"], 106, "https://github.com/celer-network/sgn-v2-contracts", [], "https://cbridge-docs.celer.network/", "https://immunefi.com/bug-bounty/celer/information/", ["immunefi", "github", "bounty-guided"]],
  ["Enzyme Protocol", "Asset Management", ["Ethereum"], 107, "https://github.com/enzymefinance/protocol", [], "https://docs.enzyme.finance/", "https://immunefi.com/bug-bounty/enzymefinance/information/", ["immunefi", "github", "bounty-guided"]],
  ["Kamino Lending", "Lending", ["Solana"], 108, "https://github.com/Kamino-Finance/klend", [], "https://docs.kamino.finance/", "https://immunefi.com/bug-bounty/kamino/information/", ["immunefi", "github", "bounty-guided"]],
  ["Kamino Vault", "Yield", ["Solana"], 109, "https://github.com/Kamino-Finance/kvault", [], "https://docs.kamino.finance/", "https://immunefi.com/bug-bounty/kamino/information/", ["immunefi", "github", "bounty-guided"]],
  ["Jito Restaking", "Restaking", ["Solana"], 110, "https://github.com/jito-foundation/restaking", [], "https://docs.jito.network/", "https://immunefi.com/bug-bounty/jito/information/", ["immunefi", "github", "bounty-guided"]],
  ["SSV Network", "Validator Infrastructure", ["Ethereum"], 111, "https://github.com/ssvlabs/ssv-network", [], "https://docs.ssv.network/", "https://immunefi.com/bug-bounty/ssvnetwork/information/", ["immunefi", "github", "bounty-guided"]],
  ["Lombard EVM Contracts", "Bitcoin LST", ["Ethereum"], 112, "https://github.com/lombard-finance/evm-smart-contracts", [], "https://docs.lombard.finance/", "https://immunefi.com/bug-bounty/lombard-finance/information/", ["immunefi", "github", "bounty-guided"]],
  ["Zest Protocol V2", "Bitcoin Lending", ["Stacks"], 113, "https://github.com/Zest-Protocol/zest-v2-contracts", [], "https://docs.zestprotocol.com/", "https://immunefi.com/bug-bounty/zest-protocol-v2/information/", ["immunefi", "github", "bounty-guided"]],
];

// Second-pass expansion from the full live Immunefi program directory. These
// repositories are linked by the program, but CYPHES deliberately records them
// as `program-linked` until a human confirms the exact current asset/path scope.
// That keeps the discovery surface broad without turning a repository link into
// an automatic bounty-eligibility claim.
const bountyExpansion = [
  ["0x Settler", "DEX Aggregator", ["Ethereum"], "https://github.com/0xProject/0x-settler", "0x"],
  ["Aave V3 Origin", "Lending", ["Ethereum"], "https://github.com/aave-dao/aave-v3-origin", "aave"],
  ["Aave GHO Origin", "Stablecoin", ["Ethereum"], "https://github.com/aave-dao/gho-origin", "aave"],
  ["Aave Governance V3", "Governance", ["Ethereum"], "https://github.com/bgd-labs/aave-governance-v3", "aave"],
  ["Aave Stake Token", "Staking", ["Ethereum"], "https://github.com/bgd-labs/stake-token", "aave"],
  ["Aera", "Treasury", ["Ethereum"], "https://github.com/aera-finance/aera-contracts-public", "aera"],
  ["Arbitrum Governance", "Governance", ["Arbitrum"], "https://github.com/ArbitrumFoundation/governance", "arbitrum"],
  ["Arbitrum Nitro Contracts", "L2", ["Ethereum", "Arbitrum"], "https://github.com/OffchainLabs/nitro-contracts", "arbitrum"],
  ["Arbitrum Token Bridge", "Bridge", ["Ethereum", "Arbitrum"], "https://github.com/OffchainLabs/token-bridge-contracts", "arbitrum"],
  ["Axelar CGP", "Bridge", ["Ethereum", "Cosmos"], "https://github.com/axelarnetwork/axelar-cgp-solidity", "axelarnetwork"],
  ["Axelar Core", "Bridge", ["Cosmos"], "https://github.com/axelarnetwork/axelar-core", "axelarnetwork"],
  ["Axelar Interchain Token Service", "Bridge", ["Ethereum", "Cosmos"], "https://github.com/axelarnetwork/interchain-token-service", "axelarnetwork"],
  ["Beanstalk", "Stablecoin", ["Ethereum"], "https://github.com/BeanstalkFarms/Beanstalk", "beanstalk"],
  ["Beanstalk Basin", "DEX", ["Ethereum"], "https://github.com/BeanstalkFarms/Basin", "beanstalk"],
  ["Beanstalk Pipeline", "Execution", ["Ethereum"], "https://github.com/BeanstalkFarms/Pipeline", "beanstalk"],
  ["Capyfi", "Lending", ["Ethereum"], "https://github.com/Capyfi/capyfi-smart-contracts", "capyfi"],
  ["Chainlink CCIP", "Bridge", ["Ethereum", "Solana"], "https://github.com/smartcontractkit/chainlink-ccip", "chainlink"],
  ["Chainlink EVM", "Oracle", ["Ethereum"], "https://github.com/smartcontractkit/chainlink-evm", "chainlink"],
  ["Chainlink Solana", "Oracle", ["Solana"], "https://github.com/smartcontractkit/chainlink-solana", "chainlink"],
  ["Chainlink Sui", "Oracle", ["Sui"], "https://github.com/smartcontractkit/chainlink-sui", "chainlink"],
  ["Chainlink libocr", "Oracle", ["Ethereum"], "https://github.com/smartcontractkit/libocr", "chainlink"],
  ["Cosmos SDK", "L1", ["Cosmos"], "https://github.com/cosmos/cosmos-sdk", "cosmos"],
  ["Cosmos EVM", "L1", ["Cosmos", "Ethereum"], "https://github.com/cosmos/evm", "cosmos"],
  ["Cosmos Gaia", "L1", ["Cosmos"], "https://github.com/cosmos/gaia", "cosmos"],
  ["IBC Go", "Bridge", ["Cosmos"], "https://github.com/cosmos/ibc-go", "cosmos"],
  ["CosmWasm", "Smart Contract Runtime", ["Cosmos"], "https://github.com/CosmWasm/cosmwasm", "cosmos"],
  ["CosmWasm wasmd", "Smart Contract Runtime", ["Cosmos"], "https://github.com/CosmWasm/wasmd", "cosmos"],
  ["CosmWasm wasmvm", "Smart Contract Runtime", ["Cosmos"], "https://github.com/CosmWasm/wasmvm", "cosmos"],
  ["IBC Hermes", "Bridge", ["Cosmos"], "https://github.com/informalsystems/hermes", "cosmos"],
  ["CoW GPv2", "DEX Aggregator", ["Ethereum"], "https://github.com/gnosis/gp-v2-contracts", "cowprotocol"],
  ["DeFi Saver V3", "Automation", ["Ethereum"], "https://github.com/defisaver/defisaver-v3-contracts", "defisaver"],
  ["DeXe Protocol", "DAO", ["Ethereum", "BNB Chain"], "https://github.com/dexe-network/DeXe-Protocol", "dexeprotocol"],
  ["Flux Finance", "Lending", ["Ethereum"], "https://github.com/flux-finance/contracts", "fluxfinance"],
  ["GMX Synthetics", "Derivatives", ["Arbitrum", "Avalanche"], "https://github.com/gmx-io/gmx-synthetics", "gmx"],
  ["Gnosis OmniBridge", "Bridge", ["Ethereum", "Gnosis"], "https://github.com/gnosischain/omnibridge", "gnosischain"],
  ["Gnosis Token Bridge", "Bridge", ["Ethereum", "Gnosis"], "https://github.com/gnosischain/tokenbridge-contracts", "gnosischain"],
  ["Immutable Contracts", "L2", ["Ethereum", "Immutable"], "https://github.com/immutable/contracts", "immutable"],
  ["Immutable zkEVM Bridge", "Bridge", ["Ethereum", "Immutable"], "https://github.com/immutable/zkevm-bridge-contracts", "immutable"],
  ["Instadapp Avocado", "Smart Account", ["Ethereum"], "https://github.com/Instadapp/avocado-contracts-public", "instadapp"],
  ["Instadapp DSA", "Smart Account", ["Ethereum"], "https://github.com/Instadapp/dsa-contracts", "instadapp"],
  ["Instadapp Fluid", "Lending", ["Ethereum"], "https://github.com/Instadapp/fluid-contracts-public", "instadapp"],
  ["Kiln Deposit Batch", "Staking", ["Ethereum"], "https://github.com/kilnfi/deposit-batch-contract", "kiln"],
  ["LayerZero Devtools", "Bridge", ["Ethereum", "Solana"], "https://github.com/LayerZero-Labs/devtools", "layerzero"],
  ["LayerZero Legacy", "Bridge", ["Ethereum"], "https://github.com/LayerZero-Labs/LayerZero", "layerzero"],
  ["Lido Community Staking Module", "Liquid Staking", ["Ethereum"], "https://github.com/lidofinance/community-staking-module", "lido"],
  ["Lido Dual Governance", "Governance", ["Ethereum"], "https://github.com/lidofinance/dual-governance", "lido"],
  ["Lido Easy Track", "Governance", ["Ethereum"], "https://github.com/lidofinance/easy-track", "lido"],
  ["Lido L2", "Liquid Staking", ["Ethereum", "Optimism"], "https://github.com/lidofinance/lido-l2", "lido"],
  ["Lido Oracle", "Oracle", ["Ethereum"], "https://github.com/lidofinance/lido-oracle", "lido"],
  ["Lista Token", "Stablecoin", ["BNB Chain"], "https://github.com/lista-dao/lista-token", "listadao"],
  ["Lista Moolah", "Lending", ["BNB Chain"], "https://github.com/lista-dao/moolah", "listadao"],
  ["Olympus", "Treasury", ["Ethereum"], "https://github.com/OlympusDAO/olympus-contracts", "olympus"],
  ["Orca Whirlpools", "DEX", ["Solana"], "https://github.com/orca-so/whirlpools", "orca"],
  ["xOrca", "DEX", ["Solana"], "https://github.com/orca-so/xorca", "orca"],
  ["Origin ARM OETH", "Yield", ["Ethereum"], "https://github.com/OriginProtocol/arm-oeth", "originprotocol"],
  ["PancakeSwap Infinity Core", "DEX", ["BNB Chain", "Ethereum"], "https://github.com/pancakeswap/infinity-core", "pancakeswap"],
  ["PancakeSwap Infinity Periphery", "DEX", ["BNB Chain", "Ethereum"], "https://github.com/pancakeswap/infinity-periphery", "pancakeswap"],
  ["PancakeSwap Infinity Router", "DEX", ["BNB Chain", "Ethereum"], "https://github.com/pancakeswap/infinity-universal-router", "pancakeswap"],
  ["Raydium AMM", "DEX", ["Solana"], "https://github.com/raydium-io/raydium-amm", "raydium"],
  ["Raydium CLMM", "DEX", ["Solana"], "https://github.com/raydium-io/raydium-amm-v3", "raydium"],
  ["Raydium CPMM", "DEX", ["Solana"], "https://github.com/raydium-io/raydium-cp-swap", "raydium"],
  ["Rhino.fi Contracts", "Bridge", ["Ethereum", "Starknet"], "https://github.com/rhinofi/contracts_public", "rhinofi"],
  ["Sei Chain", "L1", ["Sei"], "https://github.com/sei-protocol/sei-chain", "sei"],
  ["Sky DSS", "CDP", ["Ethereum"], "https://github.com/sky-ecosystem/dss", "sky"],
  ["Sky Lite PSM", "Stablecoin", ["Ethereum"], "https://github.com/sky-ecosystem/dss-lite-psm", "sky"],
  ["Sky Flash", "Flash Mint", ["Ethereum"], "https://github.com/sky-ecosystem/dss-flash", "sky"],
  ["Sky Lockstake", "Staking", ["Ethereum"], "https://github.com/sky-ecosystem/lockstake", "sky"],
  ["Sky USDS", "Stablecoin", ["Ethereum"], "https://github.com/sky-ecosystem/usds", "sky"],
  ["Spark ALM Controller", "Treasury", ["Ethereum"], "https://github.com/sparkdotfi/spark-alm-controller", "sparklend"],
  ["Spark PSM", "Stablecoin", ["Ethereum"], "https://github.com/sparkdotfi/spark-psm", "sparklend"],
  ["Spark ALM Controller", "Asset Management", ["Ethereum"], "https://github.com/sparkdotfi/spark-alm-controller", "sparklend"],
  ["Spark Vaults V2", "Yield", ["Ethereum"], "https://github.com/sparkdotfi/spark-vaults-v2", "sparklend"],
  ["Stader ETHx", "Liquid Staking", ["Ethereum"], "https://github.com/stader-labs/ethx", "staderforeth"],
  ["Veda Boring Vault", "Vault", ["Ethereum"], "https://github.com/Veda-Labs/boring-vault", "veda"],
  ["Wormhole Native Token Transfers", "Bridge", ["Ethereum", "Solana"], "https://github.com/wormhole-foundation/native-token-transfers", "wormhole"],
];

for (const [index, [name, category, chains, repoUrl, programSlug]] of bountyExpansion.entries()) {
  protocols.push([
    name,
    category,
    chains,
    114 + index,
    repoUrl,
    [],
    `https://immunefi.com/bug-bounty/${programSlug}/information/`,
    `https://immunefi.com/bug-bounty/${programSlug}/information/`,
    ["immunefi", "github", "bounty-guided", "program-linked"],
  ]);
}

const historicalRepositories = new Set([
  "https://github.com/compound-finance/compound-protocol",
  "https://github.com/euler-xyz/euler-contracts",
  "https://github.com/liquity/dev",
  "https://github.com/ribbon-finance/ribbon-v2",
  "https://github.com/yearn/yearn-vaults",
].map((url) => url.toLowerCase()));

const supersededRepositories = new Set([
  "https://github.com/compound-finance/compound-protocol",
  "https://github.com/euler-xyz/euler-contracts",
  "https://github.com/liquity/dev",
  "https://github.com/ribbon-finance/ribbon-v2",
].map((url) => url.toLowerCase()));

const financialImpactCategories = [
  "Direct theft or loss of user funds",
  "Permanent or temporary freezing of funds",
  "Protocol insolvency or materially incorrect accounting",
  "Governance result manipulation",
  "Griefing with measurable protocol or user impact",
];

const scopeAssumptions = [
  "Unsupported, fee-on-transfer, rebasing, or malicious tokens are not assumed in scope unless the program explicitly permits them or the attacker creates the condition.",
  "Privileged, governance, or administrator behavior is not assumed malicious unless the attacker can obtain or alter that privilege through the reported exploit.",
  "CI and configuration hardening is tracked separately from smart-contract bounty eligibility.",
];

function slug(value) {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
}

function creditBudget(rank, criticality) {
  return Math.max(55, Math.round(220 - Math.min(rank, 80) * 1.7 + criticality * 18));
}

const targets = [];
for (const [name, category, chains, tvlRiskRank, repoUrl, paths, docsUrl, securityUrl, sources = sourceMix] of protocols) {
  const baseCriticality = Math.max(1, 6 - Math.floor((tvlRiskRank - 1) / 10));
  const bountyGuided = sources.includes("bounty-guided");
  const repositoryScopeStatus = bountyGuided
    ? sources.includes("program-linked")
      ? "program-linked-needs-human-confirmation"
      : "explicit-program-repository"
    : "guardian-curated-needs-human-confirmation";
  const normalizedRepoUrl = repoUrl.toLowerCase();
  const implementationStatus = supersededRepositories.has(normalizedRepoUrl)
    ? "superseded"
    : historicalRepositories.has(normalizedRepoUrl)
      ? "historical"
      : "current-at-snapshot";
  const reviewClass = ["Compiler", "Execution Client", "Protocol", "Consensus", "L1", "Validator Infrastructure", "Smart Contract Runtime"].includes(category)
    ? "protocol-code"
    : "smart-contract";
  const selected = ["", ...(paths.length > 0 ? paths : [])];
  for (const [index, path] of selected.entries()) {
    const pathLabel = path || "repository";
    const targetId = `guardian-${String(tvlRiskRank).padStart(3, "0")}-${slug(name)}-${slug(pathLabel)}`;
    const criticality = Math.min(6, baseCriticality + (index === 1 ? 1 : 0));
    targets.push({
      targetId,
      protocolName: name,
      source: sources,
      category,
      chains,
      tvlRiskRank,
      repoUrl,
      repoUrls: [repoUrl],
      contractPaths: path ? [path] : [],
      docsUrl,
      securityUrl,
      bountyProgramUrl: bountyGuided ? securityUrl : null,
      bountyScopeCapturedAt: bountyGuided ? "2026-08-01T16:00:00Z" : null,
      repositoryScopeStatus,
      implementationStatus,
      reviewClass,
      impactCategories: repositoryScopeStatus === "explicit-program-repository"
        ? financialImpactCategories
        : [],
      assumptionsOutOfScope: scopeAssumptions,
      inScopeText: path
        ? `Public read-only review of ${path} at the pinned commit.`
        : "Public read-only review of repository security posture at the pinned commit.",
      outOfScopeText:
        "No production interaction, no exploit execution against live systems, no repository writes, no bounty submission, no claims of affiliation.",
      lastAuditedCommit: null,
      lastObservedCommit: null,
      contractCriticality: criticality,
      priorityScore: Math.max(10, 100 - tvlRiskRank + criticality * 6 - index * 3),
      scopeText: [
        path ? `Focused path: ${path}` : "Focused path: repository root",
        `Protocol: ${name}`,
        `Category: ${category}`,
        `Chains: ${chains.join(", ")}`,
        `Static TVL/risk rank seed: ${tvlRiskRank}`,
        `Criticality: ${criticality}/6`,
        "Public DeFi guardian coverage.",
        "No repository writes, no code execution, no production interaction.",
      ].join("\n"),
      auditBrief: [
        "CYPHES Guardian Index v2 autonomous coverage.",
        `Review ${name} ${path ? `focused path ${path}` : "repository root"} for evidence-backed security observations, coverage gaps, and verification-ready notes.`,
        "Prioritize externally verifiable evidence and uncertainty over speculative exploit claims.",
        "Do not submit externally.",
      ].join(" "),
      creditBudget: creditBudget(tvlRiskRank, criticality),
      cadence: "commit-diff-watch",
      tags: Array.from(new Set(["defi", slug(category), ...chains.slice(0, 3).map(slug), path.endsWith(".sol") ? "solidity" : "repository"])),
    });
  }
}

targets.sort((a, b) => b.priorityScore - a.priorityScore || a.tvlRiskRank - b.tvlRiskRank);

const index = {
  version: "0.17.7",
  label: "CYPHES Guardian Index v2",
  generatedAt: "2026-08-01T16:00:00Z",
  policy,
  notes: [
    "Bundled static seed for autonomous public guardian coverage.",
    "DeFiLlama is used as a risk-ranking source signal only; GitHub targets are manually curated and resolved to pinned commits by the app before work is created.",
    "Immunefi and Cantina public programs guide additional repository selection; live bounty scope and eligibility still require human review.",
    "Every bounty-guided target records a dated scope status; program-linked repositories remain discovery-only until a human confirms exact current asset and path scope.",
    "No external bounty submission or protocol contact occurs in auto mode.",
  ],
  targets,
};

writeFileSync(
  new URL("../protocol/targets/guardian-target-index.json", import.meta.url),
  `${JSON.stringify(index, null, 2)}\n`,
);

console.log(`wrote ${index.targets.length} guardian targets`);
