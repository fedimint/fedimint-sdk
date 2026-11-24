import { createContext, useContext } from 'react'

/**
 * Closes the Send sheet as a whole, from any screen in it: the stack inside the sheet would only
 * pop back to the form.
 */
export const CloseSheet = createContext<() => void>(() => {})

export const useCloseSheet = () => useContext(CloseSheet)
