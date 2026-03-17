import { Router } from 'express';
import * as userController from '../controllers/userController';

const router = Router();

// POST /api/users/register
// Мы используем POST, так как создаем ресурс
router.post('/register', userController.registerUser);

export default router;