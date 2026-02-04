// server/config/db.js
const { Pool } = require('pg');

const pool = new Pool({
    user: 'admin',
    host: 'db', // Ім'я контейнера в Docker
    database: 'social_db',
    password: 'admin1234',
    port: 5432,
});

module.exports = pool;