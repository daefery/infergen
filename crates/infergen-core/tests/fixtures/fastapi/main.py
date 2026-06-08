from fastapi import FastAPI, HTTPException
from pydantic import BaseModel

app = FastAPI()


class UserCreate(BaseModel):
    username: str
    email: str
    password: str


class LoginRequest(BaseModel):
    email: str
    password: str


@app.get("/users")
async def list_users():
    return [{"id": 1, "username": "alice"}]


@app.post("/users", status_code=201)
async def create_user(payload: UserCreate):
    return {"id": 2, **payload.dict()}


@app.get("/users/{user_id}")
async def get_user(user_id: int):
    if user_id != 1:
        raise HTTPException(status_code=404)
    return {"id": user_id, "username": "alice"}


@app.post("/auth/login")
async def login(payload: LoginRequest):
    if payload.password != "secret":
        raise HTTPException(status_code=401)
    return {"token": "jwt-token-here"}
