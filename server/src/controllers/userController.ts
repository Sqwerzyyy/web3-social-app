import { Request, Response } from 'express';
import * as userService from '../services/userService';

export const registerUser = async (req: Request, res: Response) => {
    try {
        const user = await userService.createUserProfile(req.body);
        res.status(201).json(user);
    } catch (error: any) {
        res.status(400).json({ error: error.message });
    }
};

export const loginUser = async (req: Request, res: Response) => {
    try {
        const { email, password } = req.body;
        const user = await userService.loginUser({ email, password });
        res.status(200).json(user);
    } catch (error: any) {
        if (error.message === 'User not found' || error.message === 'Invalid password') {
            return res.status(401).json({ error: 'Invalid credentials' });
        }
        res.status(500).json({ error: error.message });
    }
};