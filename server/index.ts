// server/index.ts
import express from 'express';
import cors from 'cors';
import userRoutes from "./routes/userRoutes";

const app = express();
const PORT: number = 3000;

// Middleware (прошарки)
app.use(cors());
app.use(express.json());

// Підключаємо маршрути
// Тепер всі адреси будуть починатися з /api. Наприклад: /api/register
app.use('/api', userRoutes);

app.listen(PORT, () => {
    console.log(`🚀 Сервер запущено на порту ${PORT}`);
});
