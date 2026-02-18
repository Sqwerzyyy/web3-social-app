import { useState, useRef, useEffect } from 'react'
import { motion, AnimatePresence } from 'framer-motion'
import emailjs from '@emailjs/browser'
import { useMultiStepForm } from '../hooks/useMultiStepForm'
import PremiumSelect from './PremiumSelect'

const EMAILJS_CONFIG = {
  SERVICE_ID: 'service_vycwn59',   // 🔹 замените на ваш ID сервиса
  TEMPLATE_ID: 'template_cxoznqo', // 🔹 шаблон с полем {{code}}
  PUBLIC_KEY: 'XH5OJFrPI8LOhdqgw'    // 🔹 ваш публичный ключ
}

// если хочешь оставить разработческий обход (например, на случай тестов)
const MASTER_BYPASS_CODE = '123456'

export default function RegistrationModal({
                                            isOpen,
                                            onClose,
                                            mode = 'register',
                                            showToastNotification,
                                            onLoginSuccess
                                          }) {
  const { state, setField, setErrors, setLoading, nextStep, prevStep, reset } = useMultiStepForm()
  const [generatedCode, setGeneratedCode] = useState(null)
  const [loginData, setLoginData] = useState({ email: '', password: '' })
  const [loginErrors, setLoginErrors] = useState({})
  const otpInputRef = useRef(null)

  useEffect(() => {
    if (state.currentStep === 2 && otpInputRef.current) {
      setTimeout(() => otpInputRef.current?.focus(), 300)
    }
  }, [state.currentStep])

  // === LOGIN HANDLER ===
  const handleLogin = async (e) => {
    e.preventDefault()
    setLoginErrors({})
    const errors = {}
    if (!loginData.email || !/\S+@\S+\.\S+/.test(loginData.email)) errors.email = 'Invalid Email Format'
    if (!loginData.password || loginData.password.length < 6) errors.password = 'Min 6 characters'

    if (Object.keys(errors).length > 0) {
      setLoginErrors(errors)
      return
    }

    setLoading(true)
    setTimeout(() => {
      setLoading(false)
      if (onLoginSuccess) onLoginSuccess()
    }, 1500)
  }

  // === REGISTRATION STEP 1: SEND CODE ===
  const handleSendCode = async (e) => {
    e.preventDefault()
    setErrors({})
    if (!state.data.email || !/\S+@\S+\.\S+/.test(state.data.email)) {
      setErrors({ email: 'Invalid Email Format' })
      return
    }

    setLoading(true)

    const code = Math.floor(100000 + Math.random() * 900000).toString()
    setGeneratedCode(code)

    try {
      await emailjs.send(
          EMAILJS_CONFIG.SERVICE_ID,
          EMAILJS_CONFIG.TEMPLATE_ID,
          {
            to_email: state.data.email,
            code,                // используется в шаблоне EmailJS как {{code}}
          },
          EMAILJS_CONFIG.PUBLIC_KEY
      )

      showToastNotification('Verification code sent to your email', 'success')
      nextStep()
    } catch (error) {
      console.error('Email send failed:', error)
      setErrors({ email: 'Failed to send code. Try again.' })
    } finally {
      setLoading(false)
    }
  }

  // === REGISTRATION STEP 2: VERIFY CODE ===
  const handleVerifyCode = async (e) => {
    e.preventDefault()
    setErrors({})

    const inputCode = state.data.verificationCode?.trim()
    if (!inputCode || inputCode.length !== 6) {
      setErrors({ verificationCode: 'Enter 6-digit code' })
      return
    }

    if (inputCode !== generatedCode && inputCode !== MASTER_BYPASS_CODE) {
      setErrors({ verificationCode: 'Invalid code' })
      return
    }

    setLoading(true)
    setTimeout(() => {
      setLoading(false)
      nextStep()
    }, 800)
  }

  // === REGISTRATION STEP 3: COMPLETE PROFILE ===
  const handleCompleteRegistration = async (e) => {
    e.preventDefault()
    setErrors({})
    const errors = {}

    if (!state.data.firstName.trim()) errors.firstName = 'Required'
    if (!state.data.lastName.trim()) errors.lastName = 'Required'
    if (!state.data.country) errors.country = 'Required'
    if (!state.data.nickname.trim()) errors.nickname = 'Required'
    if (!state.data.password || state.data.password.length < 8) errors.password = 'Min 8 chars'

    if (Object.keys(errors).length > 0) {
      setErrors(errors)
      return
    }

    const payload = {
      username: state.data.nickname,
      firstName: state.data.firstName,
      lastName: state.data.lastName,
      country: state.data.country,
      email: state.data.email,
      password: state.data.password
    }

    console.log('Payload prepared:', payload)

    setLoading(true)
    setTimeout(() => {
      setLoading(false)
      if (onLoginSuccess) onLoginSuccess()
    }, 1500)
  }

  const handleClose = () => {
    reset()
    setGeneratedCode(null)
    setLoginData({ email: '', password: '' })
    setLoginErrors({})
    onClose()
  }

  if (!isOpen) return null

  return (
      <motion.div className="modal-overlay" onClick={handleClose} initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}>
        <motion.div className="modal-content" onClick={(e) => e.stopPropagation()} initial={{ scale: 0.95, opacity: 0 }} animate={{ scale: 1, opacity: 1 }}>
          <button className="modal-close" onClick={handleClose}>×</button>

          <AnimatePresence mode="wait">
            {mode === 'login' ? (
                // === LOGIN ===
                <motion.div key="login" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}>
                  <h2 className="modal-title">Welcome Back</h2>
                  <form className="modal-form" onSubmit={handleLogin} noValidate>
                    <div className="form-group">
                      <input type="email" placeholder="Email Address" className={`modal-input ${loginErrors.email ? 'error' : ''}`} value={loginData.email} onChange={(e) => setLoginData({ ...loginData, email: e.target.value })} disabled={state.isLoading} autoFocus />
                      {loginErrors.email && <span className="form-error">{loginErrors.email}</span>}
                    </div>
                    <div className="form-group">
                      <input type="password" placeholder="Password" className={`modal-input ${loginErrors.password ? 'error' : ''}`} value={loginData.password} onChange={(e) => setLoginData({ ...loginData, password: e.target.value })} disabled={state.isLoading} />
                      {loginErrors.password && <span className="form-error">{loginErrors.password}</span>}
                    </div>
                    <button type="submit" className="modal-submit" disabled={state.isLoading}>Log In</button>
                  </form>
                </motion.div>
            ) : state.currentStep === 1 ? (
                // === STEP 1: EMAIL INPUT ===
                <motion.div key="step1" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}>
                  <h2 className="modal-title">Create Account</h2>
                  <p className="modal-subtitle">Enter your email to get started</p>
                  <form className="modal-form" onSubmit={handleSendCode} noValidate>
                    <div className="form-group">
                      <input type="email" placeholder="Email Address" className={`modal-input ${state.errors.email ? 'error' : ''}`} value={state.data.email} onChange={(e) => setField('email', e.target.value)} disabled={state.isLoading} autoFocus />
                      {state.errors.email && <span className="form-error">{state.errors.email}</span>}
                    </div>
                    <button type="submit" className="modal-submit" disabled={state.isLoading}>Send Verification Code</button>
                  </form>
                </motion.div>
            ) : state.currentStep === 2 ? (
                // === STEP 2: CODE VERIFICATION ===
                <motion.div key="step2" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}>
                  <h2 className="modal-title">Verify Email</h2>
                  <p className="modal-subtitle">Enter the 6-digit code sent to your email</p>
                  <form className="modal-form" onSubmit={handleVerifyCode} noValidate>
                    <div className="form-group">
                      <input ref={otpInputRef} type="text" placeholder="000000" className={`modal-input otp-input ${state.errors.verificationCode ? 'error' : ''}`} value={state.data.verificationCode} onChange={(e) => setField('verificationCode', e.target.value)} maxLength={6} disabled={state.isLoading} />
                      {state.errors.verificationCode && <span className="form-error">{state.errors.verificationCode}</span>}
                    </div>
                    <button type="submit" className="modal-submit" disabled={state.isLoading}>Verify</button>
                  </form>
                </motion.div>
            ) : (
                // === STEP 3: PROFILE COMPLETION ===
                <motion.div key="step3" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}>
                  <h2 className="modal-title">Complete Profile</h2>
                  <form className="modal-form" onSubmit={handleCompleteRegistration} noValidate>
                    <div className="form-row">
                      <div className="form-group"><input type="text" placeholder="First Name" className="modal-input" value={state.data.firstName} onChange={(e) => setField('firstName', e.target.value)} /></div>
                      <div className="form-group"><input type="text" placeholder="Last Name" className="modal-input" value={state.data.lastName} onChange={(e) => setField('lastName', e.target.value)} /></div>
                    </div>
                    <div className="form-group"><PremiumSelect value={state.data.country} onChange={(val) => setField('country', val)} placeholder="Select Country" /></div>
                    <div className="form-group"><input type="text" placeholder="Nickname" className="modal-input" value={state.data.nickname} onChange={(e) => setField('nickname', e.target.value)} /></div>
                    <div className="form-group"><input type="password" placeholder="Password" className="modal-input" value={state.data.password} onChange={(e) => setField('password', e.target.value)} /></div>
                    <button type="submit" className="modal-submit" disabled={state.isLoading}>Complete Registration</button>
                  </form>
                </motion.div>
            )}
          </AnimatePresence>
        </motion.div>
      </motion.div>
  )
}
// === ОБЩАЯ ФУНКЦИЯ ОТПРАВКИ КОДА ===
const sendVerificationEmail = async (email) => {
  setLoading(true);
  const code = Math.floor(100000 + Math.random() * 900000).toString();
  setGeneratedCode(code);

  try {
    await emailjs.send(
        EMAILJS_CONFIG.SERVICE_ID,
        EMAILJS_CONFIG.TEMPLATE_ID,
        {
          to_email: email,
          code,
        },
        EMAILJS_CONFIG.PUBLIC_KEY
    );
    showToastNotification('Verification code sent to your email', 'success');
    nextStep(); // Переходим к вводу кода (Step 2)
  } catch (error) {
    console.error('Email send failed:', error);
    setErrors({ email: 'Failed to send code. Try again.' });
    setLoginErrors({ email: 'Failed to send code.' });
  } finally {
    setLoading(false);
  }
};

// === ОБНОВЛЕННЫЙ LOGIN HANDLER ===
const handleLogin = async (e) => {
  e.preventDefault();
  setLoginErrors({});

  if (!loginData.email || !/\S+@\S+\.\S+/.test(loginData.email)) {
    setLoginErrors({ email: 'Invalid Email Format' });
    return;
  }

  // Сохраняем email в общее состояние формы, чтобы Step 2 знал, куда мы шлем код
  setField('email', loginData.email);

  // Отправляем код вместо мгновенного входа
  await sendVerificationEmail(loginData.email);
};

// === ОБНОВЛЕННЫЙ REGISTRATION STEP 1 ===
const handleSendCode = async (e) => {
  e.preventDefault();
  setErrors({});
  if (!state.data.email || !/\S+@\S+\.\S+/.test(state.data.email)) {
    setErrors({ email: 'Invalid Email Format' });
    return;
  }
  await sendVerificationEmail(state.data.email);
};

// === ОБНОВЛЕННЫЙ VERIFY CODE (Step 2) ===
const handleVerifyCode = async (e) => {
  e.preventDefault();
  setErrors({});

  const inputCode = state.data.verificationCode?.trim();
  if (inputCode !== generatedCode && inputCode !== MASTER_BYPASS_CODE) {
    setErrors({ verificationCode: 'Invalid code' });
    return;
  }

  setLoading(true);
  setTimeout(() => {
    setLoading(false);
    if (mode === 'login') {
      // Если это вход — завершаем процесс
      if (onLoginSuccess) onLoginSuccess();
    } else {
      // Если регистрация — идем на шаг заполнения профиля (Step 3)
      nextStep();
    }
  }, 800);
};