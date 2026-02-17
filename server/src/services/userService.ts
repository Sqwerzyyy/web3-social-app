// src/services/userService.ts

import bcrypt from 'bcrypt';
import jwt from 'jsonwebtoken';
import { Prisma } from '@prisma/client';
import { prisma } from '../config/prisma';
import { config } from '../config/env';
import { RegisterUserDto, LoginUserDto, AuthResponse } from '../types/user.types';
import { ValidationError, AuthError, ConflictError, ErrorCode } from '../types/errors';

const SALT_ROUNDS = 12;



const validateRegisterDto = (data: RegisterUserDto): void => {
    if (!EMAIL_REGEX.test(data.email)) {
        throw new ValidationError(ErrorCode.INVALID_EMAIL);
    }
    if (data.password.length < MIN_PASSWORD_LENGTH) {
        throw new ValidationError(ErrorCode.PASSWORD_TOO_SHORT);
    }
};

export const createUserProfile = async (data: RegisterUserDto): Promise<AuthResponse> => {
    validateRegisterDto(data);

    try {
        const user = await prisma.userProfile.create({
            data: {
                username:  data.username,
                email:     data.email.toLowerCase(),
                password:  hashedPassword,
                firstName: data.firstName ?? null,
                lastName:  data.lastName ?? null,
                country:   data.country ?? null,
            },
        });

        const { password: _password, ...userWithoutPassword } = user;
        const token = generateToken(user.id);

        return { user: userWithoutPassword, token };

    } catch (error) {

    }
};

