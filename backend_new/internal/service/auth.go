package service

import (
	"context"
	"errors"
	"web3-social-app/backend_new/internal/domain"
	"web3-social-app/backend_new/internal/repository"
)

type AuthService interface {
	Register(ctx context.Context, user *domain.User) error
	Login(ctx context.Context, email, password string) (*domain.User, error)
}

type authService struct {
	repo repository.UserRepository //юзаем ток интерфейсы
}

//принимает интерфейс - Dependency Injection.
func NewAuthService(repo repository.UserRepository) AuthService {
	return &authService{repo: repo}
}

func (s *authService) Register(ctx context.Context, user *domain.User) error {
	// тут будет логика хэша
	
	//скидываем сохранение в слой рабоыт с бд 
	return s.repo.CreateUser(ctx, user)
}

func (s *authService) Login(ctx context.Context, email, password string) (*domain.User, error) {
	//достаем юзера с бд
	user, err := s.repo.GetUserByEmail(ctx, email)
	if err != nil {
		return nil, errors.New("invalid credentials")
	}

	if user.Password != password {
		return nil, errors.New("invalid credentials")
	}

	return user, nil
}