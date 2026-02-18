import { motion, AnimatePresence } from 'framer-motion'
import { useEffect } from 'react'

export default function Toast({ message, isVisible, onClose, type = 'error' }) {
  useEffect(() => {
    if (isVisible) {
      const timer = setTimeout(() => {
        onClose()
      }, 5000)
      return () => clearTimeout(timer)
    }
  }, [isVisible, onClose])

  return (
    <AnimatePresence>
      {isVisible && (
        <motion.div
          className={`toast toast-${type}`}
          initial={{ opacity: 0, y: -21, scale: 0.95 }}
          animate={{ opacity: 1, y: 0, scale: 1 }}
          exit={{ opacity: 0, y: -20, scale: 0.95 }}
          transition={{ type: 'spring', stiffness: 300, damping: 25 }}
        >
          <div className="toast-content">
            <span className="toast-message">{message}</span>
            <button className="toast-close" onClick={onClose}>×</button>
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  )
}