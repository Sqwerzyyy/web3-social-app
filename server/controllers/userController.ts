import { Request, Response } from 'express';
import * as userService from '../services/userService';

export const registerUser = async (req: Request, res: Response) => {
    try{
        const { username, firstName, lastName, email, password, country } = req.body;

        const user = await userService.createUserProfile({
            username,
            firstName,
            lastName,
            email,
            password,
            country
        })
        res.status(201).json(user);
    } catch (error: any){
        res.status(400).json({error: error.message});
    }
}