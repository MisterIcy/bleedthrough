<?php

$value = filter_var($_GET['x'], FILTER_VALIDATE_URL);
if (in_array(parse_url($value, PHP_URL_HOST), ['api.example.com'], true)) {
    $body = file_get_contents($value);
}
