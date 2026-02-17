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

// src/validators/user.schema.ts
import { z } from 'zod';

//регистрация
export const registerUserSchema = z.object({
    body: z.object({
        email: z
            .string({ required_error: "Email is required" })
            .email("Invalid email format"),
        username: z
            .string({ required_error: "Username is required" })
            .min(3, "Username must be at least 3 chars"),
        password: z
            .string({ required_error: "Password is required" })
            .min(8, "Password must be at least 8 chars"), // Zod теперь проверяет длину
        firstName: z.string().optional(),
        lastName: z.string().optional(),
        country: z.string().optional(),
    }),
});

//логин
export const loginUserSchema = z.object({
    body: z.object({
        email: z
            .string({ required_error: "Email is required" })
            .email("Invalid email format"),
        password: z
            .string({ required_error: "Password is required" }),
    }),
});

//автоматическая генерац типв
export type RegisterUserInput = z.infer<typeof registerUserSchema>['body'];
export type LoginUserInput = z.infer<typeof loginUserSchema>['body'];