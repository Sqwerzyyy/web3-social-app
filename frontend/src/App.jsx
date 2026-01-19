import { useState } from 'react'
import { Canvas } from '@react-three/fiber'
import { PerspectiveCamera } from '@react-three/drei'
import { motion } from 'framer-motion'
import './App.css'
import TopologyNetwork from './components/TopologyVisualization'
import RegistrationModal from './components/RegistrationModal'
import Toast from './components/Toast'
import { useToast } from './hooks/useToast'

function App() {
  const [isModalOpen, setIsModalOpen] = useState(false)
  const [modalMode, setModalMode] = useState('register') // 'register' or 'login'
  const { toastMessage, showToast, toastType, showToastNotification, hideToast } = useToast()

  const openModal = (mode = 'register') => {
    setModalMode(mode)
    setIsModalOpen(true)
  }
  const closeModal = () => {
    setIsModalOpen(false)
    // Reset to register mode after closing
    setTimeout(() => setModalMode('register'), 300)
  }

  return (
    <div className="app">
      {/* Header */}
      <motion.header
        className="header"
        initial={{ y: -100, opacity: 0 }}
        animate={{ y: 0, opacity: 1 }}
        transition={{
          type: 'spring',
          stiffness: 100,
          damping: 20,
          delay: 0.1
        }}
      >
        <div className="brand">Untilted</div>
        <nav className="nav">
          <button
            onClick={() => openModal('login')}
            className="nav-link"
            type="button"
          >
            Sign In
          </button>
          <motion.button
            onClick={() => openModal('register')}
            className="register-btn"
            whileHover={{ scale: 1.02, y: -1 }}
            whileTap={{ scale: 0.98 }}
            transition={{ type: 'spring', stiffness: 400, damping: 25 }}
          >
            Register
          </motion.button>
        </nav>
      </motion.header>

      {/* Abstract Topology Visualization */}
      <div className="visualization-container">
        <Canvas
          gl={{
            antialias: true,
            alpha: true,
            powerPreference: 'high-performance'
          }}
          dpr={Math.min(window.devicePixelRatio, 2)}
        >
          <PerspectiveCamera
            makeDefault
            position={[0, 0, 15]}
            fov={60}
          />
          <ambientLight intensity={0.5} />
          <pointLight position={[10, 10, 10]} intensity={1} />
          <TopologyNetwork />
        </Canvas>
      </div>

      {/* Registration/Login Modal */}
      <RegistrationModal 
        isOpen={isModalOpen} 
        onClose={closeModal} 
        mode={modalMode}
        showToastNotification={showToastNotification}
      />

      {/* Global Toast - Fixed position at root level */}
      <Toast
        message={toastMessage}
        isVisible={showToast}
        onClose={hideToast}
        type={toastType}
      />
    </div>
  )
}

export default App
