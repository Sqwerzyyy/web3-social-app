import { PrismaClient } from '@prisma/client';

const prisma = new PrismaClient();

interface createProfileData {
    username: string;
    password: string;
    firstName: string;
    lastName: string;
    email: string;
    country: string;
}

export const createUserProfile = async (data: createProfileData) => {
    return await prisma.userProfile.create({
        data: {
            username: data.username,
            firstName: data.firstName,
            lastName: data.lastName,
            email: data.email,
            country: data.country,
        }
    })
};

