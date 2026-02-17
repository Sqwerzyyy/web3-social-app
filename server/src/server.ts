import { Server } from 'http';
import app from './app';
import { config } from './config/env';
import { prisma } from './config/prisma';


const PORT = 3000;

const server: Server = app.listen(PORT, () => {
    console.log(` Server running at http://localhost:${PORT}/api`);

    console.log(`Mode: ${process.env.NODE_ENV ?? 'development'}`);
});

