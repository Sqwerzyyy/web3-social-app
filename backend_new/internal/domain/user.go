package domain

type User struct {
	ID 		   int		`json:"id"`
	Firstname  string   `json:"firstname"`
	SecondName string   `json:"secondname"`
	Email      string   `json:"email"`
	Username   string   `json:"username"`
	Password   string   `json:"password"`
	Country    string   `json:"country"`
}