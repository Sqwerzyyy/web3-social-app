import React, { useState, useEffect } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import '../App.css';

const HomePage = () => {
    const [textStage, setTextStage] = useState('hub');

    useEffect(() => {
        // Текст держится 4 секунды, чтобы насладиться эффектом
        const timer = setTimeout(() => {
            setTextStage('none');
        }, 4000);
        return () => clearTimeout(timer);
    }, []);

    const variants = {
        // ПОЯВЛЕНИЕ: Мягкое проявление из размытия и уменьшенного масштаба
        hidden: {
            opacity: 0,
            scale: 0.9,
            y: 20,
            filter: "blur(10px)" // Просто блюр, без яркости
        },
        visible: {
            opacity: 1,
            scale: 1,
            y: 0,
            filter: "blur(0px)", // Убираем блюр, CSS подхватывает свечение
            transition: {
                duration: 1.2,
                ease: "easeOut"
            }
        },
        // ИСЧЕЗНОВЕНИЕ: Плавное растворение и увеличение
        exit: {
            opacity: 0,
            scale: 1.1,
            filter: "blur(20px)",
            transition: { duration: 0.8, ease: "easeInOut" }
        }
    };

    return (
        <div className="home-container">
            <div className="home-content">
                <AnimatePresence mode="wait">
                    {textStage === 'hub' && (
                        <motion.h1
                            key="text-hub"
                            className="crypto-title" // Применяем новый CSS класс
                            variants={variants}
                            initial="hidden"
                            animate="visible"
                            exit="exit"
                        >
                            WELCOME TO YOUR HUB
                        </motion.h1>
                    )}
                </AnimatePresence>
            </div>
        </div>
    );
};

export default HomePage;