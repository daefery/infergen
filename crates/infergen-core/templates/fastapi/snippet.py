# After `infergen generate`, import your typed SDK:
from infergen_generated import track

@app.middleware("http")
async def tracking_middleware(request: Request, call_next):
    track.endpoint_called(path=request.url.path, method=request.method)
    return await call_next(request)

@app.post("/users")
async def register(user: UserCreate):
    new_user = await create_user(user)
    track.user_registered(user_id=str(new_user.id))
    return new_user
