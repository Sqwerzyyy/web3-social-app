import React, { useState, useEffect } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import '../App.css';

const HomePage = () => {
    const [textStage, setTextStage] = useState('hub');
    const [isSettingsOpen, setIsSettingsOpen] = useState(false);

    // Настройки темы и языка
    const [settings, setSettings] = useState({
        theme: 'dark',
        notifications: true,
        language: 'UA'
    });

    useEffect(() => {
        const timer = setTimeout(() => {
            setTextStage('none');
        }, 4000);
        return () => clearTimeout(timer);
    }, []);

    // Варианты анимации
    const variants = {
        hidden: { opacity: 0, scale: 0.9, y: 20, filter: "blur(10px)" },
        visible: {
            opacity: 1, scale: 1, y: 0, filter: "blur(0px)",
            transition: { duration: 1.2, ease: "easeOut" }
        },
        exit: {
            opacity: 0, scale: 1.1, filter: "blur(20px)",
            transition: { duration: 0.8, ease: "easeInOut" }
        }
    };

    // Переводы
    const translations = {
        EN: {
            welcome: "WELCOME TO YOUR HUB",
            settings: "Settings",
            theme: "Theme",
            language: "Language",
            dark: "🌙 Dark",
            light: "☀️ Light",
            ukrainian: "Ukrainian",
            english: "English"
        },
        UA: {
            welcome: "ЛАСКАВО ПРОСИМО ДО ВАШОГО ЦЕНТРУ",
            settings: "Налаштування",
            theme: "Тема",
            language: "Мова",
            dark: "🌙 Темна",
            light: "☀️ Світла",
            ukrainian: "Українська",
            english: "Англійська"
        }
    };

    const t = translations[settings.language];

    return (
        <div className={`home-container ${settings.theme}`}>
            {/* Кнопка налаштувань */}
            <div className="top-right-nav">
                <motion.button
                    whileHover={{ rotate: 90 }}
                    whileTap={{ scale: 0.9 }}
                    className="settings-toggle"
                    onClick={() => setIsSettingsOpen(!isSettingsOpen)}
                >
                    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <path d="M12.22 2h-.44a2 2 0 0 0-2 2 2 2 0 0 1-2 2 2 2 0 0 0-2 2 2 2 0 0 1-2 2 2 2 0 0 0-2 2v.44a2 2 0 0 0 2 2 2 2 0 0 1 2 2 2 2 0 0 0 2 2 2 2 0 0 1 2 2 2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2 2 2 0 0 1 2-2 2 2 0 0 0 2-2 2 2 0 0 1 2-2 2 2 0 0 0 2-2v-.44a2 2 0 0 0-2-2 2 2 0 0 1-2-2 2 2 0 0 0-2-2 2 2 0 0 1-2-2 2 2 0 0 0-2-2zM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z"/>
                    </svg>
                </motion.button>
            </div>

            {/* Панель налаштувань */}
            <AnimatePresence>
                {isSettingsOpen && (
                    <motion.div
                        initial={{ opacity: 0, x: 300 }}
                        animate={{ opacity: 1, x: 0 }}
                        exit={{ opacity: 0, x: 300 }}
                        className="settings-panel"
                    >
                        <div className="settings-header">
                            <h3>{t.settings}</h3>
                            <button onClick={() => setIsSettingsOpen(false)}>×</button>
                        </div>
                        <div className="settings-content">
                            <div className="setting-item">
                                <span>{t.theme}</span>
                                <button
                                    onClick={() =>
                                        setSettings({...settings, theme: settings.theme === 'dark' ? 'light' : 'dark'})
                                    }
                                >
                                    {settings.theme === 'dark' ? t.dark : t.light}
                                </button>
                            </div>
                            <div className="setting-item">
                                <span>{t.language}</span>
                                <select
                                    value={settings.language}
                                    onChange={(e) => setSettings({...settings, language: e.target.value})}
                                >
                                    <option value="UA">{t.ukrainian}</option>
                                    <option value="EN">{t.english}</option>
                                </select>
                            </div>
                        </div>
                    </motion.div>
                )}
            </AnimatePresence>

            <div className="home-content">
                <AnimatePresence mode="wait">
                    {textStage === 'hub' && (
                        <motion.h1
                            key="text-hub"
                            className="crypto-title"
                            variants={variants}
                            initial="hidden"
                            animate="visible"
                            exit="exit"
                        >
                            {t.welcome}
                        </motion.h1>
                    )}
                </AnimatePresence>
            </div>
        </div>
    );
};

export default HomePage;
