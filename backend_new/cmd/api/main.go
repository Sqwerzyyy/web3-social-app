package main

import (
	"database/sql"
	"log"
	"net/http"
	"web3-social-app/backend_new/internal/handler"
	"web3-social-app/backend_new/internal/repository"
	"web3-social-app/backend_new/internal/service"
    _ "github.com/lib/pq" 
)

func main() {
	db, err := sql.Open("postgres", "postgres://user:pass@localhost:5432/dbname?sslmode=disable")
	if err != nil {
		log.Fatal(err)
	}
	defer db.Close() //точно закроет конектс с базой 

	//dependecy inj 
	userRepo := repository.NewPostgresRepository(db)   //репа получает бд
	authSvc := service.NewAuthService(userRepo)        //сервис получает репо
	authHandler := handler.NewAuthHandler(authSvc)     //хендлер получает сервис

	http.HandleFunc("/register", authHandler.Register)
	http.HandleFunc("/login", authHandler.Login)

	// 4. Запуск сервера
	log.Println("Server is running on :8080")
	log.Fatal(http.ListenAndServe(":8080", nil))
}