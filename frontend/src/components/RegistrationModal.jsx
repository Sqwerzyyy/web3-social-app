import { useState, useRef, useEffect } from 'react'
import { motion, AnimatePresence } from 'framer-motion'
import emailjs from '@emailjs/browser'
import { useMultiStepForm } from '../hooks/useMultiStepForm'
import PremiumSelect from './PremiumSelect'

// ============================================================================
// EMAILJS CONFIGURATION
// ============================================================================
// To enable real email delivery, you need to:
// 1. Sign up at https://www.emailjs.com/
// 2. Create a service (Gmail, Outlook, etc.)
// 3. Create an email template with variables: {{to_email}} and {{message}}
// 4. Get your Public Key from the Integration page
// 5. Replace the placeholders below with your actual values:
// ============================================================================

const EMAILJS_CONFIG = {
  // TODO: Replace with your EmailJS Service ID
  SERVICE_ID: 'YOUR_SERVICE_ID',
  // TODO: Replace with your EmailJS Template ID
  TEMPLATE_ID: 'YOUR_TEMPLATE_ID',
  // TODO: Replace with your EmailJS Public Key
  PUBLIC_KEY: 'YOUR_PUBLIC_KEY'
}

// Master Bypass Code - Always works even if email fails
const MASTER_BYPASS_CODE = '123456'

// ============================================================================
// END EMAILJS CONFIGURATION
// ============================================================================

export default function RegistrationModal({ isOpen, onClose, mode = 'register', showToastNotification }) {
  const {
    state,
    setField,
    setErrors,
    setLoading,
    nextStep,
    prevStep,
    reset
  } = useMultiStepForm()

  const [generatedCode, setGeneratedCode] = useState(null)
  const [loginData, setLoginData] = useState({ email: '', password: '' })
  const otpInputRef = useRef(null)

  // Auto-focus OTP input when step 2 becomes active
  useEffect(() => {
    if (state.currentStep === 2 && otpInputRef.current) {
      setTimeout(() => {
        otpInputRef.current?.focus()
      }, 300)
    }
  }, [state.currentStep])

  // Generate random 6-digit code
  const generateVerificationCode = () => {
    return Math.floor(100000 + Math.random() * 900000).toString()
  }

  const handleClose = () => {
    reset()
    setGeneratedCode(null)
    setLoginData({ email: '', password: '' })
    onClose()
  }

  // Login handler
  const handleLogin = async (e) => {
    e.preventDefault()
    
    if (!loginData.email || !/\S+@\S+\.\S+/.test(loginData.email)) {
      showToastNotification('⚠️ Invalid Email Format', 'error')
      return
    }

    if (!loginData.password || loginData.password.length < 6) {
      showToastNotification('Password must be at least 6 characters', 'error')
      return
    }

    setLoading(true)
    
    // Simulate login API call
    setTimeout(() => {
      setLoading(false)
      showToastNotification('Access Granted', 'success')
      
      // Close modal after success toast
      setTimeout(() => {
        handleClose()
      }, 2000)
    }, 2000)
  }

  // Step 1: Email submission with EmailJS integration
  const handleSendCode = async (e) => {
    e.preventDefault()
    setErrors({})
    
    if (!state.data.email || !/\S+@\S+\.\S+/.test(state.data.email)) {
      showToastNotification('⚠️ Invalid Email Format', 'error')
      return
    }

    setLoading(true)
    
    // Generate verification code
    const code = generateVerificationCode()
    setGeneratedCode(code)

    try {
      // Check if EmailJS is configured
      const isEmailJSConfigured = 
        EMAILJS_CONFIG.SERVICE_ID !== 'YOUR_SERVICE_ID' &&
        EMAILJS_CONFIG.TEMPLATE_ID !== 'YOUR_TEMPLATE_ID' &&
        EMAILJS_CONFIG.PUBLIC_KEY !== 'YOUR_PUBLIC_KEY'

      if (isEmailJSConfigured) {
        // Attempt to send email via EmailJS
        // NOTE: Replace these values with your actual EmailJS credentials
        await emailjs.send(
          EMAILJS_CONFIG.SERVICE_ID,
          EMAILJS_CONFIG.TEMPLATE_ID,
          {
            to_email: state.data.email,
            message: `Your verification code is: ${code}`
          },
          EMAILJS_CONFIG.PUBLIC_KEY
        )
        
        setLoading(false)
        nextStep()
      } else {
        // EmailJS not configured - simulate delay then proceed with bypass message
        setTimeout(() => {
          setLoading(false)
          showToastNotification('System: Email service not configured. Use developer code 123456.')
          nextStep()
        }, 1500)
      }
    } catch (error) {
      // EmailJS failed - show toast but still allow master code
      console.error('EmailJS Error:', error)
      setLoading(false)
      showToastNotification('System: Email service not configured. Use developer code 123456.')
      nextStep()
    }
  }

  // Step 2: OTP verification with master bypass code
  const handleVerifyCode = async (e) => {
    e.preventDefault()
    setErrors({})
    
    const inputCode = state.data.verificationCode?.trim()
    
    if (!inputCode || inputCode.length !== 6) {
      showToastNotification('Please enter a valid 6-digit code')
      return
    }

    // Check against generated code OR master bypass code
    const isValidCode = inputCode === generatedCode || inputCode === MASTER_BYPASS_CODE

    if (!isValidCode) {
      showToastNotification('Invalid code. Please try again or use developer code: 123456')
      return
    }

    setLoading(true)
    // Simulate verification delay
    setTimeout(() => {
      setLoading(false)
      nextStep()
    }, 800)
  }

  // Step 3: Profile creation
  const handleCompleteRegistration = async (e) => {
    e.preventDefault()
    setErrors({})
    
    const errors = {}
    if (!state.data.firstName.trim()) errors.firstName = 'First name is required'
    if (!state.data.lastName.trim()) errors.lastName = 'Last name is required'
    if (!state.data.country) errors.country = 'Please select a country'
    if (!state.data.nickname.trim()) errors.nickname = 'Nickname is required'
    if (!state.data.password || state.data.password.length < 8) {
      errors.password = 'Password must be at least 8 characters'
    }

    if (Object.keys(errors).length > 0) {
      setErrors(errors)
      // Show first error as toast
      const firstError = Object.values(errors)[0]
      showToastNotification(firstError)
      return
    }

    setLoading(true)
    // Simulate API call
    setTimeout(() => {
      setLoading(false)
      handleClose()
      // In production, show success message or redirect
    }, 1500)
  }

  const stepVariants = {
    enter: (direction) => ({
      x: direction > 0 ? 20 : -20,
      opacity: 0
    }),
    center: {
      x: 0,
      opacity: 1
    },
    exit: (direction) => ({
      x: direction > 0 ? -20 : 20,
      opacity: 0
    })
  }

  const transition = {
    type: 'spring',
    stiffness: 300,
    damping: 30
  }

  if (!isOpen) return null

  return (
    <motion.div
      className="modal-overlay"
      onClick={handleClose}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.3 }}
    >
      <motion.div
        className="modal-content"
        onClick={(e) => e.stopPropagation()}
        initial={{ scale: 0.95, opacity: 0 }}
        animate={{ scale: 1, opacity: 1 }}
        exit={{ scale: 0.95, opacity: 0 }}
        transition={transition}
      >
        <button className="modal-close" onClick={handleClose}>×</button>

        {/* Step Indicator - Only show for registration mode */}
        {mode === 'register' && (
          <div className="modal-step-indicator">
            {[1, 2, 3].map((step) => (
              <div
                key={step}
                className={`step-dot ${state.currentStep >= step ? 'active' : ''} ${
                  state.currentStep === step ? 'current' : ''
                }`}
              />
            ))}
          </div>
        )}

        <AnimatePresence mode="wait" custom={1}>
          {mode === 'login' ? (
            <motion.div
              key="login"
              custom={1}
              variants={stepVariants}
              initial="enter"
              animate="center"
              exit="exit"
              transition={transition}
            >
              <h2 className="modal-title">Welcome Back</h2>
              <p className="modal-subtitle">Sign in to continue</p>
              <form className="modal-form" onSubmit={handleLogin} noValidate>
                <div className="form-group">
                  <input
                    type="email"
                    placeholder="Email Address"
                    className="modal-input"
                    value={loginData.email}
                    onChange={(e) => setLoginData({ ...loginData, email: e.target.value })}
                    disabled={state.isLoading}
                    autoFocus
                  />
                </div>
                <div className="form-group">
                  <input
                    type="password"
                    placeholder="Password"
                    className="modal-input"
                    value={loginData.password}
                    onChange={(e) => setLoginData({ ...loginData, password: e.target.value })}
                    disabled={state.isLoading}
                  />
                </div>
                <button
                  type="submit"
                  className="modal-submit"
                  disabled={state.isLoading}
                >
                  {state.isLoading ? (
                    <span className="button-with-spinner">
                      <span className="spinner"></span>
                      Logging in...
                    </span>
                  ) : (
                    'Log In'
                  )}
                </button>
                <a href="#" className="forgot-password-link" onClick={(e) => e.preventDefault()}>
                  Forgot Password?
                </a>
              </form>
            </motion.div>
          ) : state.currentStep === 1 && (
            <motion.div
              key="step1"
              custom={1}
              variants={stepVariants}
              initial="enter"
              animate="center"
              exit="exit"
              transition={transition}
            >
              <h2 className="modal-title">Create Account</h2>
              <p className="modal-subtitle">Enter your email to get started</p>
              <form className="modal-form" onSubmit={handleSendCode} noValidate>
                <div className="form-group">
                  <input
                    type="email"
                    placeholder="Email Address"
                    className="modal-input"
                    value={state.data.email}
                    onChange={(e) => setField('email', e.target.value)}
                    disabled={state.isLoading}
                    autoFocus
                  />
                </div>
                <button
                  type="submit"
                  className="modal-submit"
                  disabled={state.isLoading}
                >
                  {state.isLoading ? (
                    <span className="button-with-spinner">
                      <span className="spinner"></span>
                      Sending...
                    </span>
                  ) : (
                    'Send Verification Code'
                  )}
                </button>
              </form>
            </motion.div>
          )}

          {state.currentStep === 2 && (
            <motion.div
              key="step2"
              custom={1}
              variants={stepVariants}
              initial="enter"
              animate="center"
              exit="exit"
              transition={transition}
            >
              <h2 className="modal-title">Verify Email</h2>
              <p className="modal-subtitle">
                We've sent a 6-digit code to {state.data.email}
                <br />
                <span className="helper-text">(Use code: 123456 if email doesn't arrive)</span>
              </p>
              <form className="modal-form" onSubmit={handleVerifyCode} noValidate>
                <div className="form-group">
                  <input
                    ref={otpInputRef}
                    type="text"
                    inputMode="numeric"
                    placeholder="000000"
                    className="modal-input otp-input"
                    value={state.data.verificationCode}
                    onChange={(e) => {
                      const value = e.target.value.replace(/\D/g, '').slice(0, 6)
                      setField('verificationCode', value)
                    }}
                    maxLength={6}
                    disabled={state.isLoading}
                    autoComplete="one-time-code"
                  />
                </div>
                <button
                  type="submit"
                  className="modal-submit"
                  disabled={state.isLoading}
                >
                  {state.isLoading ? (
                    <span className="button-with-spinner">
                      <span className="spinner"></span>
                      Verifying...
                    </span>
                  ) : (
                    'Verify & Continue'
                  )}
                </button>
                <button
                  type="button"
                  className="modal-back"
                  onClick={prevStep}
                  disabled={state.isLoading}
                >
                  ← Back
                </button>
              </form>
            </motion.div>
          )}

          {state.currentStep === 3 && (
            <motion.div
              key="step3"
              custom={1}
              variants={stepVariants}
              initial="enter"
              animate="center"
              exit="exit"
              transition={transition}
            >
              <h2 className="modal-title">Complete Profile</h2>
              <p className="modal-subtitle">Fill in your details to finish registration</p>
              <form className="modal-form" onSubmit={handleCompleteRegistration} noValidate>
                <div className="form-row">
                  <div className="form-group">
                    <input
                      type="text"
                      placeholder="First Name"
                      className="modal-input"
                      value={state.data.firstName}
                      onChange={(e) => setField('firstName', e.target.value)}
                      disabled={state.isLoading}
                    />
                  </div>
                  <div className="form-group">
                    <input
                      type="text"
                      placeholder="Last Name"
                      className="modal-input"
                      value={state.data.lastName}
                      onChange={(e) => setField('lastName', e.target.value)}
                      disabled={state.isLoading}
                    />
                  </div>
                </div>
                <div className="form-group">
                  <PremiumSelect
                    value={state.data.country}
                    onChange={(value) => setField('country', value)}
                    placeholder="Select Country"
                  />
                </div>
                <div className="form-group">
                  <input
                    type="text"
                    placeholder="Nickname"
                    className="modal-input"
                    value={state.data.nickname}
                    onChange={(e) => setField('nickname', e.target.value)}
                    disabled={state.isLoading}
                  />
                </div>
                <div className="form-group">
                  <input
                    type="password"
                    placeholder="Password"
                    className="modal-input"
                    value={state.data.password}
                    onChange={(e) => setField('password', e.target.value)}
                    disabled={state.isLoading}
                  />
                </div>
                <button
                  type="submit"
                  className="modal-submit"
                  disabled={state.isLoading}
                >
                  {state.isLoading ? (
                    <span className="button-with-spinner">
                      <span className="spinner"></span>
                      Completing...
                    </span>
                  ) : (
                    'Complete Registration'
                  )}
                </button>
                <button
                  type="button"
                  className="modal-back"
                  onClick={prevStep}
                  disabled={state.isLoading}
                >
                  ← Back
                </button>
              </form>
            </motion.div>
          )}
        </AnimatePresence>
      </motion.div>
    </motion.div>
  )
}

