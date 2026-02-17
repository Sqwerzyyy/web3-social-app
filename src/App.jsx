import { useState, useEffect, Suspense } from 'react'
import { Canvas } from '@react-three/fiber'
import { PerspectiveCamera } from '@react-three/drei'
import { motion, AnimatePresence } from 'framer-motion'
import './App.css'
import TopologyVisualization from './components/TopologyVisualization'
import RegistrationModal from './components/RegistrationModal'
import Toast from './components/Toast'
import HomePage from './components/HomePage'
import { useToast } from './hooks/useToast'

function App() {
    const [isModalOpen, setIsModalOpen] = useState(false)
    const [modalMode, setModalMode] = useState('register')
    const [isLoggedIn, setIsLoggedIn] = useState(window.location.hash === '#home')
    const { toastMessage, showToast, toastType, showToastNotification, hideToast } = useToast()

    useEffect(() => {
        const handleHashChange = () => setIsLoggedIn(window.location.hash === '#home');
        window.addEventListener('hashchange', handleHashChange);
        return () => window.removeEventListener('hashchange', handleHashChange);
    }, []);

    const openModal = (mode = 'register') => {
        setModalMode(mode);
        setIsModalOpen(true);
    }

    const closeModal = () => setIsModalOpen(false);

    const handleLoginSuccess = () => {
        setIsModalOpen(false);
        setTimeout(() => window.location.hash = 'home', 300);
    }

    return (
        <div className="app">
            <div className="visualization-container">
                <Canvas gl={{ antialias: true, alpha: true }}>
                    <PerspectiveCamera makeDefault position={[0, 10, 20]} fov={60} />
                    {/* Suspense скрывает компонент пока грузится текстура, предотвращая появление "серых квадратов" */}
                    <Suspense fallback={null}>
                        <TopologyVisualization />
                    </Suspense>
                </Canvas>
            </div>

            <div className="content-layer">
                <AnimatePresence mode="wait">
                    {isLoggedIn ? (
                        <HomePage key="home" />
                    ) : (
                        <motion.div key="landing" style={{ width: '100%', height: '100%' }} initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}>
                            <motion.header className="header" initial={{ y: -50, opacity: 0 }} animate={{ y: 0, opacity: 1 }}>
                                <div className="brand">Untilted</div>
                                <nav className="nav">
                                    <button onClick={() => openModal('login')} className="nav-link">Sign In</button>
                                    <button onClick={() => openModal('register')} className="register-btn">Register</button>
                                </nav>
                            </motion.header>
                        </motion.div>
                    )}
                </AnimatePresence>
            </div>

            <RegistrationModal isOpen={isModalOpen} onClose={closeModal} mode={modalMode} showToastNotification={showToastNotification} onLoginSuccess={handleLoginSuccess} />
            <Toast message={toastMessage} isVisible={showToast} onClose={hideToast} type={toastType} />
        </div>
    )
}

export default App