// After `infergen generate`, inject and call the typed SDK:
import { track } from "./infergen.generated";

@Controller("users")
export class UsersController {
  @Post()
  create(@Body() dto: CreateUserDto) {
    track.userCreated({ role: dto.role });
    return this.usersService.create(dto);
  }
}
