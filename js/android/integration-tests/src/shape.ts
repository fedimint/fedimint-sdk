// The federation's module generation, as scripts/setup_test_shell.sh set it up.
//
// Capability booleans cannot tell the generations apart — v1 and v2 federations
// both report ecash, lightning and onchain — so tests that care which one they
// are talking to read the shape from here, and FederationService checks it
// against the module kinds the federation itself reports.

export type Shape = 'v1' | 'v2'

/** The kinds `FederationPreview.modules` must contain for each shape. */
export const EXPECTED_MODULES: Record<Shape, readonly string[]> = {
  v1: ['mint', 'ln', 'wallet'],
  v2: ['mintv2', 'lnv2', 'walletv2'],
}

/**
 * The shape this run's federation was started with.
 *
 * Throws rather than defaulting: a test that assumed v1 against a v2
 * federation would fail on a fee or a state it never expected, far from the
 * real cause. run-android-e2e.sh always passes it; set FM_SDK_SHAPE yourself
 * when driving the runner by hand.
 */
export function currentShape(): Shape {
  const shape = process.env.FM_SDK_SHAPE
  if (shape === 'v1' || shape === 'v2') return shape
  throw new Error(
    `FM_SDK_SHAPE is "${shape ?? ''}" — expected v1 or v2. ` +
      'Run under scripts/setup_test_shell.sh (which sets it), or export it to match the federation.',
  )
}
