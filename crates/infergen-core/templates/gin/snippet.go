// After `infergen generate`, import your typed SDK:
import infergen "github.com/your-org/your-project/infergen/generated"

func TrackingMiddleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		infergen.Track.HttpRequestHandled(c.Request.Method, c.FullPath())
		c.Next()
	}
}

func LoginHandler(c *gin.Context) {
	userID := authenticate(c)
	infergen.Track.UserAuthenticated(userID)
	c.JSON(200, gin.H{"ok": true})
}
