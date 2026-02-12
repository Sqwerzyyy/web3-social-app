import { Server } from 'http';
import app from './app';
import { config } from './config/env';
import { prisma } from './config/prisma';


const PORT = config.port ?? 3000;

const server: Server = app.listen(PORT, () => {
    console.log(` Server running at http://localhost:${PORT}/api`);

    console.log(`Mode: ${process.env.NODE_ENV ?? 'development'}`);
});

const gracefulShutdown = async (signal: string) => {
    console.log(`\n${signal} received. Starting graceful shutdown...`);


    setTimeout(() => {
        console.error('Forced shutdown after timeout.');
        process.exit(1);
    }, 10000).unref();


    server.close(() => {
        console.log('HTTP server closed.');
    });

    try {
        await prisma.$disconnect();
        console.log('Database connection closed.');
        process.exit(0);
    } catch (err) {
        console.error('Error during database disconnection:', err);
        process.exit(1);
    }
};


process.on('SIGTERM', () => { void gracefulShutdown('SIGTERM'); });
process.on('SIGINT',  () => { void gracefulShutdown('SIGINT'); });

process.on('unhandledRejection', (reason) => {
    console.error('Unhandled Promise Rejection:', reason);
    void gracefulShutdown('UNHANDLED_REJECTION');
});

process.on('uncaughtException', (error) => {
    console.error('Uncaught Exception:', error);
    void gracefulShutdown('UNCAUGHT_EXCEPTION');
});