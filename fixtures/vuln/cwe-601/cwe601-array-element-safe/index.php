<?php

$params = ['x' => $_GET['x']];
if (str_starts_with($params['x'], '/') && !str_starts_with($params['x'], '//')) {
    header('Location: ' . $params['x']);
}
