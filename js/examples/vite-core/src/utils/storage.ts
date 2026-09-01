/**
 * Safely clears browser storage (OPFS and IndexedDB) used by the Fedimint WASM client.
 * This must be called BEFORE initializing the transport/worker to prevent file lock errors.
 */
export const clearClientStorage = async (): Promise<void> => {
  // 1. Wipe OPFS Database
  try {
    if (
      typeof navigator !== 'undefined' &&
      navigator.storage &&
      navigator.storage.getDirectory
    ) {
      const root = await navigator.storage.getDirectory()
      await root.removeEntry('fedimint.db')
      console.log('OPFS database fedimint.db wiped successfully.')
    }
  } catch (e: any) {
    if (e?.name !== 'NotFoundError') {
      console.error('Failed to wipe OPFS DB:', e)
    }
  }

  // 2. Wipe IndexedDB
  try {
    if (typeof window !== 'undefined' && window.indexedDB) {
      if (typeof window.indexedDB.databases === 'function') {
        const dbs = await window.indexedDB.databases()
        const deletePromises = dbs
          .filter((db) => db.name && db.name.includes('fedimint'))
          .map((db) => {
            return new Promise<void>((resolve, reject) => {
              const req = window.indexedDB.deleteDatabase(db.name!)
              req.onsuccess = () => resolve()
              req.onerror = () => reject(req.error)
              req.onblocked = () => resolve()
            })
          })
        await Promise.all(deletePromises)
        console.log('Fedimint IndexedDB databases wiped successfully.')
      }
    }
  } catch (e) {
    console.warn('Failed to clean up IndexedDB:', e)
  }
}
