<?php

$params = ['x' => $_GET['x']];
readfile('/var/www/files/' . $params['x']);
