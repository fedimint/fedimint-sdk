import { useCallback } from 'react'
import { useNavigation, useRouter } from 'expo-router'

type Href = Parameters<ReturnType<typeof useRouter>['push']>[0]

/**
 * `router.push`, but only from the screen in front: `push` always adds, so a second tap before
 * the new screen covers the old one would open it twice. Returns whether it pushed.
 */
export function usePush() {
  const router = useRouter()
  const navigation = useNavigation()
  return useCallback(
    (href: Href): boolean => {
      if (!navigation.isFocused()) return false
      router.push(href)
      return true
    },
    [navigation, router],
  )
}
