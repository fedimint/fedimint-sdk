import { Fragment } from 'react'
import { Activity } from './components/Activity'
import { RedeemEcash, SendEcash } from './components/Ecash'
import { Federations } from './components/Federations'
import { JoinFederation } from './components/JoinFederation'
import { GenerateLightningInvoice, SendLightning } from './components/Lightning'
import { Onchain } from './components/Onchain'
import { ParseInviteCode } from './components/ParseInviteCode'
import { RecoveryProgress } from './components/RecoveryProgress'
import { Wallet } from './components/Wallet'
import { WalletStatus } from './components/WalletStatus'
import { initialFaucetUrl } from './faucet'
import { SdkProvider, useSdk } from './sdk'

const App = () => (
  <SdkProvider initialFaucetUrl={initialFaucetUrl()}>
    <Page />
  </SdkProvider>
)

const Page = () => {
  const { session, federation, federationId } = useSdk()
  return (
    <>
      <header>
        <h1>Fedimint Web SDK Demo</h1>

        <div className="steps">
          <strong>Steps to get started:</strong>
          <ol>
            <li>
              Open or create a wallet, or recover one from its seed words (its
              storage persists across reloads)
            </li>
            <li>Join a federation with an invite code</li>
            <li>
              Generate an invoice and pay it, from another wallet or the
              devimint faucet
            </li>
            <li>
              Investigate the Browser Tools
              <ul>
                <li>Browser Console for logs</li>
                <li>Network Tab (websocket) for guardian requests</li>
                <li>Application Tab for the origin's private file system</li>
              </ul>
            </li>
          </ol>
        </div>
      </header>
      <main>
        <Wallet />

        {session && (
          <>
            <RecoveryProgress key={federationId} />
            <JoinFederation />
            <Federations />
          </>
        )}

        {/* Keyed by the federation: selecting another one mounts these sections afresh, so
            nothing shown or still pending for the last one ends up under the next. A payment
            already sent carries on in its own federation and shows in that one's Activity. */}
        {session && federation && (
          <Fragment key={federationId}>
            <WalletStatus />
            <GenerateLightningInvoice />
            <SendLightning />
            <SendEcash />
            <RedeemEcash />
            <Onchain />
            <Activity />
          </Fragment>
        )}

        {session && <ParseInviteCode />}
      </main>
    </>
  )
}

export default App
