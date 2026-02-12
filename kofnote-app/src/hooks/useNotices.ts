import { useCallback, useState } from 'react'

export type Notice = {
  id: number
  type: 'info' | 'success' | 'error'
  text: string
}

export function useNotices() {
  const [notices, setNotices] = useState<Notice[]>([])

  const pushNotice = useCallback((type: Notice['type'], text: string) => {
    const id = Date.now() + Math.floor(Math.random() * 1000)
    setNotices((prev) => [...prev, { id, type, text }])
    window.setTimeout(() => {
      setNotices((prev) => prev.filter((item) => item.id !== id))
    }, 3200)
  }, [])

  return { notices, pushNotice }
}
