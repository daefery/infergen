// After `infergen generate`, import your typed SDK:
import infergen "github.com/your-org/your-project/infergen/generated"

func TrackingMiddleware(next echo.HandlerFunc) echo.HandlerFunc {
	return func(c echo.Context) error {
		infergen.Track.HttpRequestHandled(c.Request().Method, c.Path())
		return next(c)
	}
}

func LoginHandler(c echo.Context) error {
	userID := authenticate(c)
	infergen.Track.UserAuthenticated(userID)
	return c.JSON(200, map[string]bool{"ok": true})
}
