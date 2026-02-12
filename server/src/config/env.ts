import dotenv from 'dotenv';

dotenv.config();

export const config = {
    port: process.env.PORT || 5001,
    jwt: {
        secret: process.env.JWT_SECRET || 'your-secret-key',
        expiresIn: process.env.JWT_EXPIRES_IN || '24h'
    }
};