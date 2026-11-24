import { usePush } from '../../../src/navigation/push'
import { SendScreen } from '../../../src/screens/send/SendScreen'
import { useCloseSheet } from '../../../src/navigation/sheet'

export default function SendForm() {
  const push = usePush()
  return (
    <SendScreen
      review={(rail) => push({ pathname: '/send/review', params: { rail } })}
      close={useCloseSheet()}
    />
  )
}
