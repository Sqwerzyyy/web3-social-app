import { useState, useRef, useEffect } from 'react'
import { motion, AnimatePresence } from 'framer-motion'

// Список країн (Label = що бачимо, Value = код для бази)
const countries = [
  { label: 'United States', value: 'US' },
  { label: 'United Kingdom', value: 'GB' },
  { label: 'Canada', value: 'CA' },
  { label: 'Australia', value: 'AU' },
  { label: 'Germany', value: 'DE' },
  { label: 'France', value: 'FR' },
  { label: 'Japan', value: 'JP' },
  { label: 'Singapore', value: 'SG' },
  { label: 'Switzerland', value: 'CH' },
  { label: 'Netherlands', value: 'NL' },
  { label: 'Sweden', value: 'SE' },
  { label: 'Norway', value: 'NO' },
  { label: 'Denmark', value: 'DK' },
  { label: 'Finland', value: 'FI' },
  { label: 'United Arab Emirates', value: 'AE' },
  { label: 'Hong Kong', value: 'HK' },
  { label: 'South Korea', value: 'KR' },
  { label: 'Ireland', value: 'IE' },
  { label: 'Luxembourg', value: 'LU' },
  { label: 'New Zealand', value: 'NZ' },
  { label: 'Ukraine', value: 'UA' },
].sort((a, b) => a.label.localeCompare(b.label)) // Автоматичне сортування А-Я

export default function PremiumSelect({ value, onChange, placeholder = 'Select Country' }) {
  const [isOpen, setIsOpen] = useState(false)
  const [highlightedIndex, setHighlightedIndex] = useState(-1)
  const containerRef = useRef(null)

  // Знаходимо повну назву країни за кодом
  const selectedCountry = countries.find(c => c.value === value)

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

  const handleSelect = (countryValue) => {
    onChange(countryValue)
    setIsOpen(false)
    setHighlightedIndex(-1)
  }

  return (
      <div className="premium-select-container" ref={containerRef}>
        {/* Кнопка відкриття */}
        <button
            type="button"
            className="premium-select-trigger"
            onClick={() => setIsOpen(!isOpen)}
        >
        <span className={!selectedCountry ? 'premium-select-placeholder' : ''}>
          {selectedCountry ? selectedCountry.label : placeholder}
        </span>
          <svg
              className={`premium-select-arrow ${isOpen ? 'open' : ''}`}
              width="12" height="12" viewBox="0 0 12 12" fill="none"
          >
            <path d="M3 4.5L6 7.5L9 4.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/>
          </svg>
        </button>

        {/* Випадаючий список */}
        <AnimatePresence>
          {isOpen && (
              <motion.div
                  className="premium-select-dropdown"
                  initial={{ opacity: 0, y: -10, scale: 0.95 }}
                  animate={{ opacity: 1, y: 0, scale: 1 }}
                  exit={{ opacity: 0, y: -10, scale: 0.95 }}
                  transition={{ duration: 0.2 }}
              >
                {countries.map((country, index) => (
                    <button
                        key={country.value}
                        type="button"
                        className={`premium-select-option ${value === country.value ? 'selected' : ''} ${highlightedIndex === index ? 'highlighted' : ''}`}
                        onClick={() => handleSelect(country.value)}
                        onMouseEnter={() => setHighlightedIndex(index)}
                    >
                      <span className="country-label">{country.label}</span>

                      {value === country.value && (
                          <span className="check-icon">✓</span>
                      )}
                    </button>
                ))}
              </motion.div>
          )}
        </AnimatePresence>
      </div>
  )
}