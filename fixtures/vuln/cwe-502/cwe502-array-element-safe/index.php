<?php

$params = ['x' => $_GET['x']];
$object = unserialize($params['x'], ['allowed_classes' => false]);
