import { useOperation, type OperationLike } from '../hooks'

/** One line with an operation's current state, green once it is final. */
export function OperationState({
  operation,
  names,
}: {
  operation: OperationLike | undefined
  names?: (n: number) => string
}) {
  const { state, final } = useOperation(operation, names)
  if (!operation || state === undefined) return null
  return (
    <div className={`state${final ? ' final' : ''}`}>
      {final ? 'Final: ' : 'State: '}
      {state}
    </div>
  )
}
