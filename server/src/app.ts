
import express, { Request, Response } from 'express';
import cors from 'cors';
import userRoutes from './routes/userRoutes';
import { errorHandler } from './middleware/error.handler';

const app = express();

// миддл вар

app.use(cors());
app.use(express.json({ limit: '10kb' }));

app.use('/api', userRoutes);
// 404 хендлер

app.use((_req: Request, res: Response) => {
    res.status(404).json({
        error: 'Route not found',
        code: 'NOT_FOUND',
    });
});
app.use(errorHandler);

export default app;