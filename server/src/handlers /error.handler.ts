
import { Request, Response, NextFunction } from 'express';
import { AppError, ErrorCode, ERROR_MESSAGES } from '../types/errors';

export const errorHandler = (
    error: unknown,
    _req: Request,
    res: Response,
    _next: NextFunction
): void => {
    if (error instanceof AppError) {
        res.status(error.statusCode).json({
            error: ERROR_MESSAGES[error.code],
            code:  error.code,
        });
        return;
    }
    console.error('[Unhandled Error]', error);

    res.status(500).json({
        error: ERROR_MESSAGES[ErrorCode.INTERNAL_SERVER_ERROR],
        code:  ErrorCode.INTERNAL_SERVER_ERROR,
    });
};