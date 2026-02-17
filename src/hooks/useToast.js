import { useState, useCallback } from 'react'

export const useToast = () => {
  const [toastMessage, setToastMessage] = useState(null)
  const [showToast, setShowToast] = useState(false)
  const [toastType, setToastType] = useState('error')

  const showToastNotification = useCallback((message, type = 'error') => {
    setToastMessage(message)
    setToastType(type)
    setShowToast(true)
  }, [])

  const hideToast = useCallback(() => {
    setShowToast(false)
  }, [])

  return {
    toastMessage,
    showToast,
    toastType,
    showToastNotification,
    hideToast
  }
}
