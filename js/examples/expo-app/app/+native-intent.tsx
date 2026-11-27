/**
 * Links go nowhere but the app's start: one that launches the app starts it as usual, one that
 * arrives while it runs is ignored. The scheme is only there because Expo Router needs one, and a
 * link could otherwise open any route directly, such as Review over a quote still held.
 */
export function redirectSystemPath({ initial }: { initial: boolean }) {
  return initial ? '/' : null
}
