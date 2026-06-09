Rails.application.routes.draw do
  devise_for :users, controllers: { sessions: 'users/sessions' }

  namespace :api do
    namespace :v1 do
      resources :articles, only: [:index, :show, :create, :destroy]
      resources :comments, only: [:create, :destroy]
      get 'profile', to: 'users#profile'
    end
  end

  root 'home#index'
  get 'about', to: 'pages#about'
  get 'pricing', to: 'pages#pricing'
end
