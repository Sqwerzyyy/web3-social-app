/*
import { z } from 'zod';

export const registerSchema = z.object({
    username:  z.string().min(3).max(32),
    email:     z.string().email(),
    password:  z.string().min(8).max(100),
    firstName: z.string().max(50).optional(),
    lastName:  z.string().max(50).optional(),
    country:   z.string().max(50).optional(),
});

export const loginSchema = z.object({
    email:    z.string().email(),
    password: z.string().min(1),
});

export type RegisterDto = z.infer<typeof registerSchema>;
export type LoginDto    = z.infer<typeof loginSchema>;

 */
