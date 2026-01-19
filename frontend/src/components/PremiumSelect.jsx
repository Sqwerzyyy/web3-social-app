import { useState, useRef, useEffect } from 'react'
import { motion, AnimatePresence } from 'framer-motion'

const countries = [
  'United States',
  'United Kingdom',
  'Canada',
  'Australia',
  'Germany',
  'France',
  'Japan',
  'Singapore',
  'Switzerland',
  'Netherlands',
  'Sweden',
  'Norway',
  'Denmark',
  'Finland',
  'United Arab Emirates',
  'Hong Kong',
  'South Korea',
  'Ireland',
  'Luxembourg',
  'New Zealand'
]

export default function PremiumSelect({ value, onChange, placeholder = 'Select Country' }) {
  const [isOpen, setIsOpen] = useState(false)
  const [highlightedIndex, setHighlightedIndex] = useState(-1)
  const containerRef = useRef(null)

  useEffect(() => {
    const handleClickOutside = (event) => {
      if (containerRef.current && !containerRef.current.contains(event.target)) {
        setIsOpen(false)
        setHighlightedIndex(-1)
      }
    }

    document.addEventListener('mousedown', handleClickOutside)
    return () => document.removeEventListener('mousedown', handleClickOutside)
  }, [])

  const handleSelect = (country) => {
    onChange(country)
    setIsOpen(false)
    setHighlightedIndex(-1)
  }

  const selectedIndex = countries.findIndex(c => c === value)

  return (
    <div className="premium-select-container" ref={containerRef}>
      <button
        type="button"
        className="premium-select-trigger"
        onClick={() => setIsOpen(!isOpen)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault()
            setIsOpen(!isOpen)
          } else if (e.key === 'ArrowDown' && isOpen) {
            e.preventDefault()
            setHighlightedIndex(prev => 
              prev < countries.length - 1 ? prev + 1 : prev
            )
          } else if (e.key === 'ArrowUp' && isOpen) {
            e.preventDefault()
            setHighlightedIndex(prev => prev > 0 ? prev - 1 : prev)
          } else if (e.key === 'Enter' && isOpen && highlightedIndex >= 0) {
            e.preventDefault()
            handleSelect(countries[highlightedIndex])
          }
        }}
      >
        <span className={!value ? 'premium-select-placeholder' : ''}>
          {value || placeholder}
        </span>
        <svg
          className={`premium-select-arrow ${isOpen ? 'open' : ''}`}
          width="12"
          height="12"
          viewBox="0 0 12 12"
          fill="none"
          xmlns="http://www.w3.org/2000/svg"
        >
          <path
            d="M3 4.5L6 7.5L9 4.5"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </button>

      <AnimatePresence>
        {isOpen && (
          <motion.div
            className="premium-select-dropdown"
            initial={{ opacity: 0, y: -10, scale: 0.95 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: -10, scale: 0.95 }}
            transition={{
              type: 'spring',
              stiffness: 300,
              damping: 30
            }}
          >
            {countries.map((country, index) => (
              <motion.button
                key={country}
                type="button"
                className={`premium-select-option ${
                  value === country ? 'selected' : ''
                } ${highlightedIndex === index ? 'highlighted' : ''}`}
                onClick={() => handleSelect(country)}
                onMouseEnter={() => setHighlightedIndex(index)}
                whileHover={{ x: 2 }}
                transition={{ type: 'spring', stiffness: 400, damping: 25 }}
              >
                {country}
                {value === country && (
                  <motion.svg
                    width="16"
                    height="16"
                    viewBox="0 0 16 16"
                    fill="none"
                    initial={{ scale: 0 }}
                    animate={{ scale: 1 }}
                    transition={{ type: 'spring', stiffness: 300, damping: 20 }}
                  >
                    <path
                      d="M13.5 4.5L6 12L2.5 8.5"
                      stroke="currentColor"
                      strokeWidth="2"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                    />
                  </motion.svg>
                )}
              </motion.button>
            ))}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  )
}
