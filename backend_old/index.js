// server/index.js
const express = require('express');
const cors = require('cors');
const userRoutes = require('./routes/userRoutes'); // Імпортуємо наші маршрути

const app = express();
const PORT = 3000;

// Middleware (прошарки)
app.use(cors());
app.use(express.json());

// Підключаємо маршрути
// Тепер всі адреси будуть починатися з /api. Наприклад: /api/register
app.use('/api', userRoutes);

app.listen(PORT, () => {
    console.log(`🚀 Сервер запущено на порту ${PORT}`);
});
