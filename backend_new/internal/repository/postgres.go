package repository

import (
	"context"
	"database/sql"
	"errors"
	"web3-social-app/backend_new/internal/domain"
)



type UserRepository interface {
	CreateUser(ctx context.Context, user *domain.User) error
	GetUserByEmail(ctx context.Context, email string) (*domain.User, error)
} //интерфейс под бд

//интерфейс под постгрес
type postgresRepo struct {
	db *sql.DB //пул соединений
}

//экземпляр
func NewPostgresRepository(db *sql.DB) UserRepository {
	return &postgresRepo{db: db}
}

// CreateUser сохраняет пользователя.
func (r *postgresRepo) CreateUser(ctx context.Context, user *domain.User) error {
	query := `INSERT INTO users (firstname, secondname, email, password, country) 
              VALUES ($1, $2, $3, $4, $5) RETURNING id`

	// QueryRowContext выполняет запрос и сканирует возвращенный ID прямо в структуру пользователя.
	// Здесь user.ID передается по указателю (&user.ID), чтобы функция Scan могла изменить его значение.
	err := r.db.QueryRowContext(ctx, query, user.Firstname, user.SecondName, user.Email, user.Password, user.Country).Scan(&user.ID)
	return err
}

//логика поиска 
func (r *postgresRepo) GetUserByEmail(ctx context.Context, email string) (*domain.User, error) {
	query := `SELECT id, firstname, secondname, email, password, country FROM users WHERE email = $1`

	user := &domain.User{} //пустая структура в памяти и указатель на нее

	err := r.db.QueryRowContext(ctx, query, email).Scan(
		&user.ID, &user.Firstname, &user.SecondName, &user.Email, &user.Password, &user.Country,
	)
	if err != nil {
		if err == sql.ErrNoRows {
			return nil, errors.New("user not found")
		}
		return nil, err
	}

	return user, nil
}
