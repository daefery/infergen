# After `infergen generate`, require your typed SDK:
require_relative "infergen/generated"

# In ApplicationController:
class ApplicationController < ActionController::Base
  before_action :track_action

  private

  def track_action
    Infergen::Track.controller_action_called(
      controller: controller_name,
      action: action_name
    )
  end
end

# After Devise sign-in:
def after_sign_in_path_for(resource)
  Infergen::Track.user_signed_in(user_id: current_user.id.to_s)
  root_path
end
