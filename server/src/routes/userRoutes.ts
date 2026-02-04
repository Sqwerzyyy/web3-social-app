import { Router } from 'express';
import { registerUser } from '../controllers/userController';

const router = Router();

// Метод POST, путь /register. Юзер стучится сюда — мы отправляем его в контроллер.
router.post('/register', registerUser);


export default router;