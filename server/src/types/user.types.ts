

import { Prisma, UserProfile } from '@prisma/client';

export interface RegisterUserDto {
    readonly username: string;
    readonly email: string;
    readonly password: string;
    readonly firstName?: string;
    readonly lastName?: string;
    readonly country?: string;
}

export interface LoginUserDto {
    readonly email: string;
    readonly password: string;
}


export type UserResponse = Omit<UserProfile, 'password'>;

export interface AuthResponse {
    readonly user: UserResponse;
    readonly token: string;
}


export type CreateUserInput = Prisma.UserProfileCreateInput;