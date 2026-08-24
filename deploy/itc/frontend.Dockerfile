FROM nginx:alpine
COPY client/dist /usr/share/nginx/html
COPY deploy/itc/frontend-nginx.conf /etc/nginx/conf.d/default.conf
EXPOSE 80

