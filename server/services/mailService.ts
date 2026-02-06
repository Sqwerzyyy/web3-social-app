import dotenv from "dotenv";
import nodemailer from 'nodemailer';
import {ISendMailParams} from "../types/mail/mail.types";

dotenv.config();

const transporter = nodemailer.createTransport({
    host: process.env.MAIL_HOST,
    port: Number(process.env.MAIL_PORT),
    secure: false, // STARTTLS загугли че это, false потому что такой порт
    auth: {
        user: process.env.MAIL_USER,
        pass: process.env.MAIL_PASSWORD,
    },
})

export const sendMailService = async (params: ISendMailParams) => {
    const { to, subject, html } = params;

    try {
        const info = await transporter.sendMail({
            from: process.env.SMTP_USER, // Отправитель
            to: to,                      // Получатель
            subject: subject,            // Тема
            html: html,                  // Тело письма
        });

        console.log('Email sent successfully:', info.messageId);
        return true; // Возвращаем успех

    } catch (error) {
        console.error('Error sending email:', error);
        return false; // Возвращаем неудачу
    }
};