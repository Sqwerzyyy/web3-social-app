// src/services/userService.ts

import bcrypt from 'bcrypt';
import jwt from 'jsonwebtoken';
import { Prisma } from '@prisma/client';
import { prisma } from '../config/prisma';
import { config } from '../config/env';
import { RegisterUserDto, LoginUserDto, AuthResponse } from '../types/user.types';
import { ValidationError, AuthError, ConflictError, ErrorCode } from '../types/errors';

const SALT_ROUNDS = 12;
const MIN_PASSWORD_LENGTH = 8;
const EMAIL_REGEX = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

const generateToken = (userId: number): string => {
    return jwt.sign(
        { userId },
        config.jwt.secret,
        { expiresIn: config.jwt.expiresIn as jwt.SignOptions['expiresIn'] }
    );
};

const validateRegisterDto = (data: RegisterUserDto): void => {
    if (!EMAIL_REGEX.test(data.email)) {
        throw new ValidationError(ErrorCode.INVALID_EMAIL);
    }
    if (data.password.length < MIN_PASSWORD_LENGTH) {
        throw new ValidationError(ErrorCode.PASSWORD_TOO_SHORT);
    }
};

const UNIQUE_FIELD_TO_ERROR_CODE: Record<string, ErrorCode> = {
    email:    ErrorCode.EMAIL_ALREADY_EXISTS,
    username: ErrorCode.USERNAME_ALREADY_EXISTS,
};

const handlePrismaError = (error: unknown): never => {
    if (error instanceof Prisma.PrismaClientKnownRequestError) {
        if (error.code === 'P2002') {
            const targets = error.meta?.target;
            const field = Array.isArray(targets) ? targets[0] : targets;

            if (typeof field === 'string' && field in UNIQUE_FIELD_TO_ERROR_CODE) {
                throw new ConflictError(UNIQUE_FIELD_TO_ERROR_CODE[field]);
            }

            throw new ConflictError(
                ErrorCode.VALIDATION_ERROR,
                `Unique constraint failed on field: ${field}`
            );
        }
    }
    throw error;
};

export const createUserProfile = async (
    data: RegisterUserDto
): Promise<AuthResponse> => {
    validateRegisterDto(data);

    const hashedPassword = await bcrypt.hash(data.password, SALT_ROUNDS);

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
        handlePrismaError(error);

        throw error;
    }
};

export const loginUser = async (
    { email, password }: LoginUserDto
): Promise<AuthResponse> => {
    const user = await prisma.userProfile.findUnique({
        where: { email: email.toLowerCase() },
    });

    if (!user) {
        throw new AuthError(ErrorCode.INVALID_CREDENTIALS);
    }

    const isPasswordValid = await bcrypt.compare(password, user.password);

    if (!isPasswordValid) {
        throw new AuthError(ErrorCode.INVALID_CREDENTIALS);
    }

    const { password: _password, ...userWithoutPassword } = user;
    const token = generateToken(user.id);

    return { user: userWithoutPassword, token };
};