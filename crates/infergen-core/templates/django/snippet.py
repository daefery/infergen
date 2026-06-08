# After `infergen generate`, import your typed SDK:
from infergen_generated import track

# In a view:
def my_view(request):
    track.view_requested(view_name="my_view", method=request.method)
    # ...

# In a login view:
def login_view(request):
    success = authenticate(request)
    track.user_login_attempted(success=success)
