package handler

import (
	"encoding/json"
	"net/http"
	"web3-social-app/backend_new/internal/domain"
	"web3-social-app/backend_new/internal/service"
)

type AuthHandler struct {
	service service.AuthService // Хэндлер зависит только от интерфейса бизнес-логики
}

func NewAuthHandler(service service.AuthService) *AuthHandler {
	return &AuthHandler{service: service}
}

//пост на регистрацию
func (h *AuthHandler) Register(w http.ResponseWriter, r *http.Request) {
	var user domain.User
	
	//читаем рекбади
	if err := json.NewDecoder(r.Body).Decode(&user); err != nil {
		http.Error(w, "invalid request format", http.StatusBadRequest)
		return
	}//указатель что бы декод записал данные во внутрь

	if err := h.service.Register(r.Context(), &user); err != nil {
		http.Error(w, "failed to register user", http.StatusInternalServerError)
		return
	}

	w.WriteHeader(http.StatusCreated)
	json.NewEncoder(w).Encode(map[string]string{"message": "user registered successfully"})
}

//пост на вход
func (h *AuthHandler) Login(w http.ResponseWriter, r *http.Request) {
	var creds struct {
		Email    string `json:"email"`
		Password string `json:"password"`
	}

	if err := json.NewDecoder(r.Body).Decode(&creds); err != nil {
		http.Error(w, "invalid request format", http.StatusBadRequest)
		return
	}

	user, err := h.service.Login(r.Context(), creds.Email, creds.Password)
	if err != nil {
		http.Error(w, "unauthorized", http.StatusUnauthorized)
		return
	}

	//возвращаем
	json.NewEncoder(w).Encode(user)
}