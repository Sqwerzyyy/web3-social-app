
import { Request, Response, NextFunction } from 'express';
import jwt from 'jsonwebtoken';
import { config } from '../config/env';
import { AuthError, ErrorCode } from '../types/errors';



export const ValidateAuth = (schema: AnyZodObject) => {
    (req: Request, res: Response, next: NextFunction) => {
        const check = schema.parse(req.body)
        if(!check.success) {
            return res.status(400).json(result.error)
        }
        req.body = result
        next()
    }
};