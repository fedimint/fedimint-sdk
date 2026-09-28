import { AppiumTestBase } from '../configs/appium/AppiumTestBase'

// A fixture drives the app from a fresh install to a named state (e.g.
// "joinedFederation"). `requires` lists other states that must be reached
// first — the runner topo-sorts these so a test can declare just the state
// it needs and the whole chain gets satisfied.
export interface Fixture {
  produces: string
  requires: readonly string[]
  run(t: AppiumTestBase): Promise<void>
}
