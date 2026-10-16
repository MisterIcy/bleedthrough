<?php

$params = ['x' => $_GET['x']];
if (in_array(parse_url($params['x'], PHP_URL_HOST), ['api.example.com'], true)) {
    $body = file_get_contents($params['x']);
}
