import { useEffect } from 'react'
import { useRouter } from 'expo-router'
import { model } from '../../src/model/DemoModel'
import { BackupScreen } from '../../src/screens/onboarding/BackupScreen'

/** The recovery phrase again, later: the backup screen from the wallet's menu. */
export default function RecoveryPhrase() {
  const router = useRouter()
  // Every way off the sheet, Close and swiping it away alike, so the phrase is never still
  // revealed the next time it opens.
  useEffect(() => () => model.hideSeed(), [])
  return (
    <BackupScreen
      next={() => router.back()}
      later={() => router.back()}
      step={null}
    />
  )
}
