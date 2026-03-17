// server/controllers/userController.js
const pool = require('../config/db');
const crypto = require('crypto');

// Логіка реєстрації
const registerUser = async (req, res) => {
    const { username, bio, owner_address } = req.body;

    try {
        // Хешуємо дані
        const docData = JSON.stringify({ username, bio, owner_address, timestamp: Date.now() });
        const docHash = crypto.createHash('sha256').update(docData).digest('hex');

        // Записуємо в базу
        const query = `
      INSERT INTO rwa_profiles (username, bio, owner_address, doc_hash)
      VALUES ($1, $2, $3, $4)
      RETURNING *;
    `;
        const result = await pool.query(query, [username, bio, owner_address, docHash]);

        res.json({
            message: "Профіль створено! Готовий до токенізації.",
            profile: result.rows[0]
        });
    } catch (err) {
        console.error(err);
        res.status(500).json({ error: err.message });
    }
};



// Логіка створення таблиці
const setupDatabase = async (req, res) => {
    try {
        const query = `
      CREATE TABLE IF NOT EXISTS rwa_profiles (
        id SERIAL PRIMARY KEY,
        owner_address VARCHAR(42) NOT NULL,
        username VARCHAR(50) UNIQUE NOT NULL,
        bio TEXT,
        total_shares INT DEFAULT 1000,
        doc_hash VARCHAR(64) NOT NULL,
        status VARCHAR(20) DEFAULT 'draft',
        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
      );
    `;
        await pool.query(query);
        res.send("✅ Таблиця 'rwa_profiles' готова.");
    } catch (err) {
        res.status(500).send("Помилка: " + err.message);
    }
};

module.exports = { registerUser, testConnection, setupDatabase };