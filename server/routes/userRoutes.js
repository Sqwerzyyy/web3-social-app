// server/routes/userRoutes.js
const express = require('express');
const router = express.Router();
const userController = require('../controllers/userController');

// Визначаємо маршрути
router.get('/test-db', userController.testConnection);
router.get('/setup-db', userController.setupDatabase);
router.post('/register', userController.registerUser);

module.exports = router;