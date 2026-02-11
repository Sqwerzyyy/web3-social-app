import { PrismaClient } from '@prisma/client';
import bcrypt from 'bcrypt';

const prisma = new PrismaClient();

interface LoginParams {
    email: string;
    password: string;
}
export const createUserProfile = async (data: any) => {
    const salt = await bcrypt.genSalt(10);
    const hashedPassword = await bcrypt.hash(data.password, salt);

    return await prisma.userProfile.create({
        data: {
            username: data.username,
            email: data.email,
            password: hashedPassword,
            firstName: data.firstName || '',
            lastName: data.lastName || '',
            country: data.country || ''
        }
    });
};
export const loginUser = async ({ email, password }: LoginParams) => {
    const user = await prisma.userProfile.findUnique({
        where: { email }
    });
    if (!user) {
        throw new Error('User not found');
    }
    const isPasswordValid = await bcrypt.compare(password, user.password);

    if (!isPasswordValid) {
        throw new Error('Invalid password');
    }
    const { password: _, ...userWithoutPassword } = user;
    return userWithoutPassword;
};

