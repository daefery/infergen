# After `infergen generate`, import your typed SDK:
from infergen_generated import track

@app.before_request
def track_request():
    track.route_accessed(endpoint=request.endpoint or "", method=request.method)

@app.route("/login", methods=["POST"])
def login():
    user = authenticate(request.form)
    if user:
        track.user_logged_in(user_id=str(user.id))
    return redirect("/dashboard")
