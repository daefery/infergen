from django.shortcuts import render, get_object_or_404
from django.contrib.auth.decorators import login_required
from django.views import View
from django.http import JsonResponse
from .models import Article


@login_required
def article_list(request):
    articles = Article.objects.all()
    return render(request, 'articles/list.html', {'articles': articles})


@login_required
def article_detail(request, pk):
    article = get_object_or_404(Article, pk=pk)
    return render(request, 'articles/detail.html', {'article': article})


class ArticleCreateView(View):
    def get(self, request):
        return render(request, 'articles/create.html')

    def post(self, request):
        title = request.POST.get('title')
        body = request.POST.get('body')
        article = Article.objects.create(title=title, body=body, author=request.user)
        return JsonResponse({'id': article.pk})
