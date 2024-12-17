import type { DefaultTheme } from 'vitepress'

export function getSidebar() {
  return [
    {
      base: '/core/',
      collapsed: false,
      text: 'Introduction',
      items: [
        { text: 'Overview', link: 'overview' },
        { text: 'Getting Started', link: 'getting-started' },
        { text: 'Architecture', link: 'architecture' },
      ],
    },
    ...FedimintWalletSidebar,
    {
      base: '/examples/',
      collapsed: false,
      text: 'Examples',
      items: [
        { text: 'Vite + React', link: 'vite-react' },
        { text: 'Vanilla JS', link: 'bare-js' },
      ],
    },
    {
      text: 'Dev',
      collapsed: true,
      base: '/core/dev/',
      items: [
        { text: 'Contributing', link: 'contributing' },
        { text: 'Awesome Projects', link: 'awesome' },
        { text: 'Testing', link: 'testing' },
      ],
    },
  ] satisfies DefaultTheme.Sidebar
}

const FedimintWalletSidebar = [
  {
    text: 'Core',
    collapsed: true,
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
          { text: 'payInvoice()', link: 'payInvoice' },
          { text: 'createInvoice()', link: 'createInvoice' },
          // {
          //   text: 'createInvoiceWithGateway()',
          //   link: 'createInvoiceWithGateway',
          // },
          // { text: 'subscribeInvoiceStatus()', link: 'subscribeInvoiceStatus' },
          // { text: 'subscribeLnReceive()', link: 'subscribeLnReceive' },
          // { text: 'listGateways()', link: 'listGateways' },
          // { text: 'getGateway()', link: 'getGateway' },
          // { text: 'updateGatewayCache()', link: 'updateGatewayCache' },
        ],
      },
      {
        text: 'MintService',
        base: '/core/FedimintWallet/MintService/',
        items: [
          { text: 'redeemEcash()', link: 'redeemEcash' },
          { text: 'spendNotes()', link: 'spendNotes' },
          { text: 'parseNotes()', link: 'parseNotes' },
        ],
      },
      {
        text: 'FederationService',
        base: '/core/FedimintWallet/FederationService/',
        items: [
          { text: 'getConfig()', link: 'getConfig' },
          { text: 'getFederationId()', link: 'getFederationId' },
          { text: 'getInviteCode()', link: 'getInviteCode' },
        ],
      },
      {
        text: 'RecoveryService',
        base: '/core/FedimintWallet/RecoveryService/',
        items: [{ text: 'Docs TODO' }],
      },
    ],
  },
  // {
  //   text: 'Type Aliases',
  //   collapsed: true,
  //   base: '/core/type-aliases/',
  //   items: [
  //     { text: 'CreateResponse', link: 'CreateResponse' },
  //     { text: 'FeeToAmount', link: 'FeeToAmount' },
  //     { text: 'LightningGateway', link: 'LightningGateway' },
  //     { text: 'LnPayState', link: 'LnPayState' },
  //     { text: 'OutgoingLightningPayment', link: 'OutgoingLightningPayment' },
  //     { text: 'PayType', link: 'PayType' },
  //     { text: 'RouteHint', link: 'RouteHint' },
  //   ],
  // },
]
