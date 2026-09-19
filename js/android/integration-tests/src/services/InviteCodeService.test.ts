/* eslint-disable no-console */
import { AppiumTestBase } from '../configs/appium/AppiumTestBase'

// Second federation-free test: `InviteCode.parse` + `federationId()`, which
// the example app exposes in its own section. Parsing an invite code talks to no
// guardian, so this stays offline like MnemonicService — but it covers a
// different shape of the binding surface: a value type constructed from a
// string, rather than the SDK handle itself.
//
// It also exercises the harness's multi-test path: the runner resets the app
// between tests whose declared state differs (see MnemonicService.produces).
export class InviteCodeService extends AppiumTestBase {
  async execute(): Promise<void> {
    console.log('Starting InviteCodeService test')

    await this.waitForText('Fedimint Android SDK Demo', 0, true, 30000)

    // The example app pre-fills its Join section with a known federation's invite
    // code. Reading it back from there rather than hardcoding one here keeps
    // the code in a single place (MainActivity's TESTNET_FEDERATION_CODE).
    // A ScrollView only renders what is on screen, so scroll it into the tree
    // before reading rather than assuming a screen tall enough.
    await this.scrollToElement('invite')
    const inviteCode = (await this.getTextByKey('invite')).trim()
    if (!inviteCode.startsWith('fed1')) {
      throw new Error(
        `Expected the example app to pre-fill a fed1… invite code, got: "${inviteCode}"`,
      )
    }

    // The parse section sits below the fold on a phone-sized screen, and the
    // keyboard covers what is left of it once the field has focus.
    await this.scrollToElement('parseInvite')
    await this.typeIntoElementByKey('parseInvite', inviteCode)
    await this.dismissKeyboard()
    await this.scrollToElement('parseInviteButton')
    await this.clickElementByKey('parseInviteButton')

    const result = await this.waitForTextInElement(
      'parseInviteResult',
      'Fed Id',
    )

    // A federation id is the 32-byte hash of the guardian set, rendered hex.
    const federationId = result.replace('Fed Id:', '').trim()
    if (!/^[0-9a-f]{64}$/i.test(federationId)) {
      throw new Error(
        `Expected a 64-character hex federation id, got: "${federationId}"`,
      )
    }

    console.log(`InviteCodeService test passed for federation ${federationId}`)
  }
}
