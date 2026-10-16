<?php

$params = ['x' => $_GET['x']];
readfile('/var/www/files/' . basename($params['x']));
