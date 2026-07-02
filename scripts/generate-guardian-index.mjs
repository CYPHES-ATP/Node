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
for (const [name, category, chains, tvlRiskRank, repoUrl, paths, docsUrl, securityUrl] of protocols) {
  const baseCriticality = Math.max(1, 6 - Math.floor((tvlRiskRank - 1) / 10));
  const selected = ["", ...(paths.length > 0 ? paths : [])];
  for (const [index, path] of selected.entries()) {
    const pathLabel = path || "repository";
    const targetId = `guardian-${String(tvlRiskRank).padStart(3, "0")}-${slug(name)}-${slug(pathLabel)}`;
    const criticality = Math.min(6, baseCriticality + (index === 1 ? 1 : 0));
    targets.push({
      targetId,
      protocolName: name,
      source: sourceMix,
      category,
      chains,
      tvlRiskRank,
      repoUrl,
      repoUrls: [repoUrl],
      contractPaths: path ? [path] : [],
      docsUrl,
      securityUrl,
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
  version: "0.7.6",
  label: "CYPHES Guardian Index v2",
  generatedAt: "2026-07-02T00:00:00Z",
  policy,
  notes: [
    "Bundled static seed for autonomous public guardian coverage.",
    "DeFiLlama is used as a risk-ranking source signal only; GitHub targets are manually curated and resolved to pinned commits by the app before work is created.",
    "No external bounty submission or protocol contact occurs in auto mode.",
  ],
  targets: targets.slice(0, 150),
};

writeFileSync(
  new URL("../protocol/targets/guardian-target-index.json", import.meta.url),
  `${JSON.stringify(index, null, 2)}\n`,
);

console.log(`wrote ${index.targets.length} guardian targets`);
