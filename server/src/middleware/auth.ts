
import { Request, Response, NextFunction } from 'express';
import jwt from 'jsonwebtoken';
import { config } from '../config/env';
import { AuthError, ErrorCode } from '../types/errors';

interface JwtPayload {
    userId: number;
}

export const authMiddleware = (
    req: Request,
    _res: Response,
    next: NextFunction
): void => {
    const authHeader = req.headers.authorization;

    if (!authHeader?.startsWith('Bearer ')) {
        next(new AuthError(ErrorCode.UNAUTHORIZED, 'Authentication required'));
        return; //
    }

    const token = authHeader.split(' ')[1];


    if (!token) {
        next(new AuthError(ErrorCode.UNAUTHORIZED, 'Token is missing'));
        return;
    }

    try {
        const decoded = jwt.verify(token, config.jwt.secret) as JwtPayload;


        if (typeof decoded.userId !== 'number') {
            next(new AuthError(ErrorCode.UNAUTHORIZED, 'Invalid token payload'));
            return;
        }

        req.userId = decoded.userId;
        next();

    } catch (error) {
        if (error instanceof jwt.TokenExpiredError) {
            next(new AuthError(ErrorCode.UNAUTHORIZED, 'Token has expired'));
            return;
        }

        next(new AuthError(ErrorCode.FORBIDDEN, 'Invalid token'));
    }
};