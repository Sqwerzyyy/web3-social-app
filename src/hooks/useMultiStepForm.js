import { useReducer } from 'react'

// State machine for multi-step registration flow
const initialState = {
  currentStep: 1,
  totalSteps: 3,
  data: {
    email: '',
    verificationCode: '',
    firstName: '',
    lastName: '',
    country: '',
    nickname: '',
    password: ''
  },
  errors: {},
  isLoading: false
}

const formReducer = (state, action) => {
  switch (action.type) {
    case 'SET_FIELD':
      return {
        ...state,
        data: {
          ...state.data,
          [action.field]: action.value
        },
        errors: {
          ...state.errors,
          [action.field]: null
        }
      }

    case 'SET_ERRORS':
      return {
        ...state,
        errors: action.errors
      }

    case 'SET_LOADING':
      return {
        ...state,
        isLoading: action.isLoading
      }

    case 'NEXT_STEP':
      return {
        ...state,
        currentStep: Math.min(state.currentStep + 1, state.totalSteps),
        errors: {}
      }

    case 'PREV_STEP':
      return {
        ...state,
        currentStep: Math.max(state.currentStep - 1, 1),
        errors: {}
      }

    case 'GO_TO_STEP':
      return {
        ...state,
        currentStep: action.step,
        errors: {}
      }

    case 'RESET':
      return initialState

    default:
      return state
  }
}

export const useMultiStepForm = () => {
  const [state, dispatch] = useReducer(formReducer, initialState)

  const setField = (field, value) => {
    dispatch({ type: 'SET_FIELD', field, value })
  }

  const setErrors = (errors) => {
    dispatch({ type: 'SET_ERRORS', errors })
  }

  const setLoading = (isLoading) => {
    dispatch({ type: 'SET_LOADING', isLoading })
  }

  const nextStep = () => {
    dispatch({ type: 'NEXT_STEP' })
  }

  const prevStep = () => {
    dispatch({ type: 'PREV_STEP' })
  }

  const goToStep = (step) => {
    dispatch({ type: 'GO_TO_STEP', step })
  }

  const reset = () => {
    dispatch({ type: 'RESET' })
  }

  return {
    state,
    setField,
    setErrors,
    setLoading,
    nextStep,
    prevStep,
    goToStep,
    reset
  }
}
