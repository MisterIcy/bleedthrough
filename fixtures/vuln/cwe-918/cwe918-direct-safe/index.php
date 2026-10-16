<?php

if (in_array(parse_url($_GET['x'], PHP_URL_HOST), ['api.example.com'], true)) {
    $body = file_get_contents($_GET['x']);
}
