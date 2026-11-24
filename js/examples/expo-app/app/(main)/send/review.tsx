import { useEffect, useState } from 'react'
import { BackHandler } from 'react-native'
import { useLocalSearchParams, useNavigation } from 'expo-router'
import type { PaymentRail } from '../../../src/components/Status'
import { ReviewScreen } from '../../../src/screens/send/ReviewScreen'
import { useCloseSheet } from '../../../src/navigation/sheet'

/** The review, which becomes the result once the payment is sent. */
export default function Review() {
  const navigation = useNavigation()
  const done = useCloseSheet()
  const [submitted, setSubmitted] = useState(false)
  const params = useLocalSearchParams<{ rail: string }>()
  // Only the form opens this, with one of the three rails; anything else falls back to Lightning.
  const rail: PaymentRail =
    params.rail === 'ecash' || params.rail === 'onchain'
      ? params.rail
      : 'lightning'
  useEffect(() => {
    navigation.setOptions({
      title: submitted
        ? rail === 'ecash'
          ? 'Ecash notes'
          : ''
        : 'Review payment',
      // Once sent there is no going back to the quote: it is spent.
      headerBackVisible: !submitted,
      gestureEnabled: !submitted,
    })
  }, [submitted, rail, navigation])
  // Android's back button is not covered by the options above: once sent it closes the sheet
  // rather than going back to the spent quote's form.
  useEffect(() => {
    if (!submitted) return
    const subscription = BackHandler.addEventListener(
      'hardwareBackPress',
      () => {
        done()
        return true
      },
    )
    return () => subscription.remove()
  }, [submitted, done])
  return (
    <ReviewScreen
      rail={rail}
      done={done}
      back={navigation.goBack}
      submitted={submitted}
      setSubmitted={() => setSubmitted(true)}
    />
  )
}
