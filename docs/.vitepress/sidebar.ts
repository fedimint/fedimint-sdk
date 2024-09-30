import type { DefaultTheme } from 'vitepress'

export function getSidebar() {
  return {
    '/core/': [
      {
        base: '/core/',
        text: 'Introduction',
        items: [
          { text: 'Overview', link: 'overview' },
          { text: 'Getting Started', link: 'getting-started' },
          { text: 'Architecture', link: 'architecture' },
        ],
      },
      ...FedimintWalletSidebar,
      {
        text: 'Dev',
        base: '/core/dev/',
        items: [
          { text: 'Contributing', link: 'contributing' },
          { text: 'Testing', link: 'testing' },
        ],
      },
    ],
    '/examples/': [
      {
        base: '/examples/',
        text: 'Examples',
        items: [
          { text: 'Vite + React', link: 'vite-react' },
          { text: 'Vanilla JS', link: 'bare-js' },
        ],
      },
    ],
  } satisfies DefaultTheme.Sidebar
}

const FedimintWalletSidebar = [
  {
    text: 'FedimintWallet',
    link: '.',
    base: '/core/FedimintWallet/',
    items: [
      {
        text: 'new FedimintWallet()',
        link: 'constructor',
      },
      {
        text: 'setLogLevel()',
        link: 'setLogLevel',
      },
      {
        text: 'joinFederation()',
        link: 'joinFederation',
      },
      {
        text: 'initialize()',
        link: 'initialize',
      },
      {
        text: 'isOpen()',
        link: 'isOpen',
      },
      {
        text: 'open()',
        link: 'open',
      },
      {
        text: 'cleanup()',
        link: 'cleanup',
      },
      {
        text: 'BalanceService',
        base: '/core/FedimintWallet/BalanceService/',
        items: [
          { text: 'getBalance()', link: 'getBalance' },
          { text: 'subscribeBalance()', link: 'subscribeBalance' },
        ],
      },
      {
        text: 'LightningService',
        base: '/core/FedimintWallet/LightningService/',
        items: [
          { text: 'Docs TODO' },
          // { text: 'payInvoice()', link: 'payInvoice' },
          // { text: 'createInvoice()', link: 'createInvoice' },
          // {
          //   text: 'createInvoiceWithGateway()',
          //   link: 'createInvoiceWithGateway',
          // },
          // { text: 'subscribeInvoiceStatus()', link: 'subscribeInvoiceStatus' },
          // { text: 'subscribeLnPay()', link: 'subscribeLnPay' },
          // { text: 'subscribeLnReceive()', link: 'subscribeLnReceive' },
          // { text: 'listGateways()', link: 'listGateways' },
          // { text: 'getGateway()', link: 'getGateway' },
          // { text: 'updateGatewayCache()', link: 'updateGatewayCache' },
        ],
      },
      {
        text: 'MintService',
        base: '/core/FedimintWallet/MintService/',
        items: [{ text: 'Docs TODO' }],
      },
      {
        text: 'FederationService',
        base: '/core/FedimintWallet/FederationService/',
        items: [{ text: 'Docs TODO' }],
      },
      {
        text: 'RecoveryService',
        base: '/core/FedimintWallet/RecoveryService/',
        items: [{ text: 'Docs TODO' }],
      },
    ],
  },
  {
    text: 'Type Aliases',
    collapsed: true,
    base: '/core/type-aliases/',
    items: [
      { text: 'CreateResponse', link: 'CreateResponse' },
      { text: 'FeeToAmount', link: 'FeeToAmount' },
      { text: 'LightningGateway', link: 'LightningGateway' },
      { text: 'LnPayState', link: 'LnPayState' },
      { text: 'OutgoingLightningPayment', link: 'OutgoingLightningPayment' },
      { text: 'PayType', link: 'PayType' },
      { text: 'RouteHint', link: 'RouteHint' },
    ],
  },
]
