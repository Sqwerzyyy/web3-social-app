import { Request, Response, NextFunction } from 'express';
import * as userService from '../services/userService';
import { RegisterUserDto, LoginUserDto } from '../types/user.types';

type RegisterRequest = Request<{}, {}, RegisterUserDto>;
type LoginRequest = Request<{}, {}, LoginUserDto>;
export const registerUser = async (
    req: RegisterRequest,
    res: Response,
    next: NextFunction
): Promise<void> => {
    try {
        const { username, email, password, firstName, lastName, country } = req.body;

        const result = await userService.createUserProfile({
            username,
            email,
            password,
            firstName,
            lastName,
            country,
        });

        res.status(201).json({
            message: 'User registered successfully',
            data: result,
        });

    } catch (error) {
        next(error);
    }
};

export const loginUser = async (
    req: LoginRequest,
    res: Response,
    next: NextFunction
): Promise<void> => {
    try {
        const { email, password } = req.body;

        const result = await userService.loginUser({ email, password });

        res.status(200).json({
            message: 'Login successful',
            data: result,
        });

    } catch (error) {
        next(error);
    }
};